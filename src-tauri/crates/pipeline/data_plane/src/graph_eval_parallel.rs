//! 并发评估执行器 — 图内路径 (评估单元) 分块 fork-join
//!
//! 与串行 [`crate::graph_eval::process_source_batch`] 语义逐项对应, 差异仅在执行拓扑:
//! - **单元→桶**: 编译期评估单元 (见 `lower::units`: prelude + 计算连通分量)
//!   按 LPT 贪心装箱为 K 桶 (K = `PipelineConfig.eval_workers`); 批内 spawn K-1 个
//!   常驻 worker (协调者跑桶 0), 以双屏障 (评估/回放) 分块推进 — 避免 per-chunk
//!   重新 spawn 线程的固定开销 ([`barrier`])
//! - **槽位副本**: 每桶对所涉及的图各持一份槽位缓冲副本; 单元写集互斥 (编译期
//!   切分不变量), 跨单元读只指向 prelude 槽位 — 每份副本先跑 prelude 再跑本桶
//!   计算单元, 副本内自洽 ([`worker`])
//! - **读路径归桶**: 端口批/派生边/频谱槽按 "正本槽位 → 所属单元 → 桶" 分派到
//!   恰好一个桶 ([`plan`]); worker 只写私有 staging, 每块结束经 stage 交换槽交给
//!   协调者, 按确定序回放 — 派生环与主时间戳轴的 1:1 对齐、端口批与快照语义与
//!   串行版一致
//! - **SIMD 批量单元**: Math 资格单元 (全 Math + 输入仅 prelude/本单元) 在
//!   块内 SoA 批量求值 (SciRS2 SIMD, 逐位对齐标量; [`simd`]), 读路径按槽位
//!   归属延后到 scatter 回放 — 可经 `eval_simd` 关闭退回全标量
//! - **快照**: 中途发布点协调者置发布标志, worker 在块尾把本桶单元的物化增量
//!   随 staging 交换带回, 协调者合并进 snap.values ([`publish`]) — 节奏与串行版一致
//! - **共享状态零克隆交接**: `source_frames`/`source_texts`/`decoder_states`
//!   `mem::take` 整表取出共享只读 ([`crate::graph_eval::PutBack`] 保证任何退路
//!   原样写回); filter/ifft/trigger 状态按单元 id 表切分为每桶子 map, 批尾合并
//!   写回 (worker panic 时该桶子状态丢失 — 懒建语义兜底; 批次与串行版一致中断)
//! - **静态图**: `is_static_local` 的图每批评估一次 (输入批内不变, 输出值相同);
//!   派生边仍逐帧重复 push 常值保持时间轴对齐, 端口批降为批尾单样本 (常值线视觉不变)
//!
//! 锁序与串行路径一致 (input_values → custom_outputs → source_texts → graphs →
//! filter → decoder → ifft → trigger → analyzers), 批间仍互斥。

mod barrier;
mod plan;
mod publish;
mod simd;
mod worker;

use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use buffer_databuffer::DerivedWriter;
use data_bus::TopicKey;
use engine::{CompiledGraph, SourceFramesMap};
use kind::NodeKind;
use parking_lot::Mutex;

use crate::eval_state::GraphEvalState;
use crate::graph_eval::{
    graph_requires_full_batch, graph_triggered_by, records_waveform_history, EvalBreakdown,
    PutBack, SlotBufs, TakeGuard,
};

use barrier::{PanicSignal, SpinBarrier, StageSlot};
use plan::{
    build_plans, new_slot_bufs, ns_since, split_states, BatchCtx, ParallelPlans, WorkerState,
    EVAL_CHUNK,
};
use publish::{publish_batch_tail, publish_point, publish_port_batches};
use worker::{drain_worker, push_static_derived, run_bucket_chunk};

/// 并发批处理入口 — `eval_workers ≥ 2` 时由 `on_frames_detached` 调入
///
/// `source_frames` 已由调用方持锁；派生写句柄不持有原始缓冲锁。
pub(crate) fn process_source_batch_parallel(
    eval_state: &GraphEvalState,
    source_frames: &mut SourceFramesMap,
    source_id: &str,
    frames: &[vofa_core::DataFrame],
    derived: &DerivedWriter,
    eval_workers: usize,
    simd: bool,
    breakdown: &mut EvalBreakdown,
) {
    if frames.is_empty() {
        return;
    }

    // —— 锁序: 与串行路径一致 (见模块注释)
    let inputs_guard = eval_state.input_values.read();
    let customs_guard = eval_state.custom_outputs.read();
    let texts = TakeGuard::take(eval_state.source_texts.lock());
    let graphs = eval_state.graphs.lock();
    let graphs_version = eval_state.graphs_version.load(Ordering::Relaxed);
    let mut filters = TakeGuard::take(eval_state.filter_states.lock());
    let mut filters_all = std::mem::take(filters.get_mut());
    let decoders = TakeGuard::take(eval_state.decoder_states.lock());
    let mut iffts = TakeGuard::take(eval_state.ifft_states.lock());
    let mut iffts_all = std::mem::take(iffts.get_mut());
    let mut triggers = TakeGuard::take(eval_state.trigger_states.lock());
    let mut triggers_all = std::mem::take(triggers.get_mut());
    let mut analyzers = eval_state.spectrum_analyzers.lock();
    let mut sf_map = PutBack::take(source_frames);

    // 触发图划分: 静态图每批一次, 动态图进并发单元
    let mut graph_list: Vec<&CompiledGraph> = Vec::new();
    let mut static_list: Vec<&CompiledGraph> = Vec::new();
    for g in graphs.values() {
        if !graph_triggered_by(g, source_id) {
            continue;
        }
        if g.compiled().is_static_local() {
            static_list.push(g);
        } else {
            graph_list.push(g);
        }
    }

    if graph_list.is_empty() && static_list.is_empty() {
        // 无图批次仍维护 source_frames (latest-value) — 与串行路径语义一致
        // (eval_workers 默认并行后, 纯波形会话没有图也必须有最新帧缓存)
        let sf = sf_map.get_mut();
        if let Some(last) = frames.last() {
            match sf.get_mut(source_id) {
                Some(slot) => {
                    slot.timestamp = last.timestamp;
                    slot.channels.clone_from(&last.channels);
                }
                None => {
                    sf.insert(source_id.to_string(), last.clone());
                }
            }
        }
        return; // put_back 守卫落栈时原样写回
    }

    // —— 静态图: 每批评估一次 (输入批内不变; 无状态节点, 全表传参无副作用)
    let frames = if !static_list.is_empty()
        || graph_list
            .iter()
            .any(|graph| graph_requires_full_batch(graph))
    {
        frames
    } else {
        &frames[frames.len() - 1..]
    };
    let mut static_bufs: Vec<SlotBufs> = static_list
        .iter()
        .map(|g| new_slot_bufs(g.compiled()))
        .collect();
    let t = std::time::Instant::now();
    for (bufs, g) in static_bufs.iter_mut().zip(&static_list) {
        g.compiled().run(
            sf_map.get(),
            texts.get(),
            &inputs_guard,
            &customs_guard,
            &mut filters_all,
            decoders.get(),
            &mut iffts_all,
            &mut triggers_all,
            &mut bufs.0,
            &mut bufs.1,
            &mut bufs.2,
            &mut bufs.3,
        );
    }
    breakdown.graph_eval_ns += ns_since(t);

    // 静态图派生边 (正本槽位, buffer 派生索引) — 逐帧重复 push 常值保持时间轴对齐
    let mut static_edges: Vec<Vec<(usize, usize)>> = vec![Vec::new(); static_list.len()];
    for (g, edges) in static_list.iter().zip(&mut static_edges) {
        for e in g.edges().filter(|edge| records_waveform_history(g, edge)) {
            if let Some(slot) = g.compiled().slot_of(&e.source, &e.source_handle) {
                edges.push((
                    slot,
                    derived.port_index_of(&e.target, &e.source, &e.source_handle),
                ));
            }
        }
    }

    // —— 动态图派生边预计算 + 单元→桶 LPT 装箱 + 读路径归桶
    let mut derived_edges: Vec<Vec<(usize, usize)>> = vec![Vec::new(); graph_list.len()];
    for (gi, g) in graph_list.iter().enumerate() {
        for e in g.edges().filter(|edge| records_waveform_history(g, edge)) {
            if let Some(slot) = g.compiled().slot_of(&e.source, &e.source_handle) {
                derived_edges[gi].push((
                    slot,
                    derived.port_index_of(&e.target, &e.source, &e.source_handle),
                ));
            }
        }
    }
    let ParallelPlans {
        arcs: plan_arcs,
        unit_bucket,
    } = build_plans(
        &graph_list,
        eval_state,
        &mut derived_edges,
        eval_workers,
        simd,
    );

    // —— 状态切分: 按单元 id 表 drain 出每桶子 map (未认领条目留原表, 批尾合并写回)
    let worker_states = split_states(
        &plan_arcs,
        &graph_list,
        &mut filters_all,
        &mut iffts_all,
        &mut triggers_all,
    );

    // 每图触发源下标 (帧覆盖解析用)
    let trigger_src_idx: Vec<Option<usize>> = graph_list
        .iter()
        .map(|g| {
            g.compiled()
                .frame_sources()
                .iter()
                .position(|s| s == source_id)
        })
        .collect();

    // 块区间 (worker 与协调者共享同一分块)
    let mut chunks: Vec<(usize, usize)> = Vec::new();
    let mut chunk_cursor = 0;
    while chunk_cursor < frames.len() {
        let end = (chunk_cursor + EVAL_CHUNK).min(frames.len());
        chunks.push((chunk_cursor, end));
        chunk_cursor = end;
    }
    let publish_due = AtomicBool::new(false);
    let publish_interval = std::time::Duration::from_millis(8);

    let ctx = BatchCtx {
        graph_list: &graph_list,
        frames,
        sf_map: sf_map.get(),
        texts: texts.get(),
        inputs: &inputs_guard,
        customs: &customs_guard,
        decoders: decoders.get(),
        trigger_src_idx: &trigger_src_idx,
        publish_due: &publish_due,
    };

    // —— 分块推进: 每块 push_frame → 并行评估 (双屏障) → 协调者按序回放。
    // worker panic 经屏障中毒 + scope 传播, 批次一致中断; 桶状态经归还箱回收合并。
    let mut states_iter = worker_states.into_iter();
    let mut lead = states_iter.next().expect("至少一个桶");
    let lead_plan = Arc::clone(&plan_arcs[0]);
    let return_slots: Vec<Arc<Mutex<Option<WorkerState>>>> = (1..plan_arcs.len())
        .map(|_| Arc::new(Mutex::new(None)))
        .collect();
    let swap_slots: Vec<Arc<Mutex<StageSlot>>> = (1..plan_arcs.len())
        .map(|_| Arc::new(Mutex::new(StageSlot::new())))
        .collect();
    let mut last_publish = std::time::Instant::now();
    // std Barrier 无中毒机制: worker panic 时置位 broken (避免其余线程屏障死锁),
    // panic payload 经 panic_slot 传递, 批尾统一续传 (保持与串行版一致的批次中断)
    let broken = AtomicBool::new(false);
    let panic_slot: Arc<Mutex<Option<Box<dyn std::any::Any + Send>>>> = Arc::new(Mutex::new(None));

    let chunk_result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        if plan_arcs.len() == 1 {
            // 单桶: 协调者直跑 (staging 私有直 drain, 无屏障)
            for &(cs, ce) in &chunks {
                // 原始帧已由记录平面入库 (record_frames); 本层只做评估与派生回放
                let t = std::time::Instant::now();
                run_bucket_chunk(&lead_plan, &mut lead, &ctx, (cs, ce));
                breakdown.graph_eval_ns += ns_since(t);

                let t = std::time::Instant::now();
                drain_worker(&lead_plan, &mut lead, derived, &mut analyzers, breakdown);
                push_static_derived(&static_edges, &static_bufs, frames, (cs, ce), derived);
                breakdown.derived_ns += ns_since(t);

                if last_publish.elapsed() >= publish_interval {
                    publish_point(
                        eval_state,
                        &graph_list,
                        std::slice::from_ref(&lead),
                        &unit_bucket,
                        &static_list,
                        &static_bufs,
                    );
                    last_publish = std::time::Instant::now();
                }
            }
        } else {
            let eval_barrier = Arc::new(SpinBarrier::new(plan_arcs.len()));
            let drain_barrier = Arc::new(SpinBarrier::new(plan_arcs.len()));
            let chunks_ref = &chunks;
            let ctx_ref = &ctx;
            std::thread::scope(|scope| {
                let _panic_signal = PanicSignal(&broken);
                for b in 1..plan_arcs.len() {
                    let plan = Arc::clone(&plan_arcs[b]);
                    let swap_slot = Arc::clone(&swap_slots[b - 1]);
                    let slot_handle = Arc::clone(&return_slots[b - 1]);
                    let eval_barrier = Arc::clone(&eval_barrier);
                    let drain_barrier = Arc::clone(&drain_barrier);
                    let ws = states_iter.next().expect("worker 状态已预留");
                    let broken = &broken;
                    let panic_slot = Arc::clone(&panic_slot);
                    scope.spawn(move || {
                        let _panic_signal = PanicSignal(broken);
                        let mut ws = ws;
                        for &(cs, ce) in chunks_ref {
                            let done = std::panic::catch_unwind(AssertUnwindSafe(|| {
                                run_bucket_chunk(&plan, &mut ws, ctx_ref, (cs, ce));
                            }));
                            if let Err(payload) = done {
                                if let Some(mut g) = panic_slot.try_lock() {
                                    if g.is_none() {
                                        *g = Some(payload);
                                    }
                                }
                                broken.store(true, Ordering::Relaxed);
                            }
                            if ctx_ref.publish_due.load(Ordering::Relaxed)
                                && !broken.load(Ordering::Relaxed)
                            {
                                ws.snapshot_delta =
                                    Some(worker::materialize_bucket(&plan, &ws, ctx_ref));
                            }
                            {
                                let mut g = swap_slot.lock();
                                g.swap_from(&mut ws);
                            }
                            eval_barrier.wait(broken);
                            drain_barrier.wait(broken);
                            if broken.load(Ordering::Relaxed) {
                                break;
                            }
                        }
                        *slot_handle.lock() = Some(ws);
                    });
                }

                // 协调者: 桶 0 执行 + 每块屏障间回放/发布
                for &(cs, ce) in chunks_ref {
                    // 原始帧已由记录平面入库 (record_frames); 本层只做评估与派生回放
                    let t = std::time::Instant::now();
                    run_bucket_chunk(&lead_plan, &mut lead, &ctx, (cs, ce));
                    breakdown.graph_eval_ns += ns_since(t);

                    eval_barrier.wait(&broken);
                    if broken.load(Ordering::Relaxed) {
                        break;
                    }

                    let t = std::time::Instant::now();
                    drain_worker(&lead_plan, &mut lead, derived, &mut analyzers, breakdown);
                    for (b, slot) in swap_slots.iter().enumerate() {
                        let mut g = slot.lock();
                        derived.append(g.staged_derived.drain(..));
                        for (si, value) in g.staged_spectra.drain(..) {
                            let (_, sink, _) = &plan_arcs[b + 1].spectra[si as usize];
                            if let Some(analyzer) = analyzers.get_mut(sink) {
                                analyzer.push(value);
                            }
                        }
                    }
                    push_static_derived(&static_edges, &static_bufs, frames, (cs, ce), derived);
                    breakdown.derived_ns += ns_since(t);

                    // 中途快照发布: 消费 worker 物化增量 + 协调者本桶/静态图
                    if publish_due.swap(false, Ordering::Relaxed) {
                        let mut snap = eval_state.output_snapshot.lock();
                        if let Some((values, _)) = lead.snapshot_delta.take() {
                            for (node, ports) in values {
                                snap.values.insert(node, ports);
                            }
                        }
                        for slot in &swap_slots {
                            let mut g = slot.lock();
                            if let Some((values, _)) = g.snapshot_delta.take() {
                                for (node, ports) in values {
                                    snap.values.insert(node, ports);
                                }
                            }
                        }
                        for (bufs, g) in static_bufs.iter().zip(&static_list) {
                            g.compiled().materialize(&bufs.0, &bufs.1, &mut snap.values);
                        }
                        snap.tick = snap.tick.wrapping_add(1);
                        drop(snap);
                        last_publish = std::time::Instant::now();
                    }

                    drain_barrier.wait(&broken);
                    if broken.load(Ordering::Relaxed) {
                        break;
                    }
                }
            });
        }
    }));

    let chunk_error = chunk_result.err();

    // worker 归还箱: 回收桶状态 (panic 的桶用空状态占位, 保持索引对齐)
    let mut all_ws: Vec<WorkerState> = vec![lead];
    for (b, slot_handle) in return_slots.iter().enumerate() {
        let ws = slot_handle
            .lock()
            .take()
            .unwrap_or_else(|| WorkerState::new(&plan_arcs[b + 1]));
        all_ws.push(ws);
    }

    // 状态合并写回 (worker panic 的桶丢失子状态 — 懒建重建); panic 在此续传
    for ws in &mut all_ws {
        filters_all.extend(ws.filters.drain());
        iffts_all.extend(ws.iffts.drain());
        triggers_all.extend(ws.triggers.drain());
    }
    *filters.get_mut() = filters_all;
    *iffts.get_mut() = iffts_all;
    *triggers.get_mut() = triggers_all;
    let panic_payload = chunk_error.or_else(|| panic_slot.lock().take());
    if let Some(panic_payload) = panic_payload {
        std::panic::resume_unwind(panic_payload);
    }

    // —— 批尾最终发布 (与串行版同语义; DataBus 端口批见 publish_port_batches)
    publish_batch_tail(
        eval_state,
        graphs_version,
        &graph_list,
        &all_ws,
        &unit_bucket,
        &static_list,
        &static_bufs,
    );

    drop(analyzers);
    drop(triggers);
    drop(iffts);

    // 触发源最新帧写回缓存 (latest-value 融合 — 与串行版逐帧覆盖的批尾效果一致)
    if let Some(last) = frames.last() {
        match sf_map.get_mut().get_mut(source_id) {
            Some(slot) => {
                slot.timestamp = last.timestamp;
                slot.channels.clone_from(&last.channels);
            }
            None => {
                sf_map.get_mut().insert(source_id.to_string(), last.clone());
            }
        }
    }

    // —— DataBus 端口批: 静态图批尾单样本收集 (锁内) → 与动态批一起发布
    let mut static_publish: Vec<(TopicKey, u64, f64)> = Vec::new();
    if let Some(last) = frames.last() {
        for (bufs, g) in static_bufs.iter().zip(&static_list) {
            let compiled = g.compiled();
            for (slot, (node_id, port)) in compiled.slot_names().iter().enumerate() {
                if matches!(
                    g.value_def(node_id).map(|node| &node.kind),
                    Some(NodeKind::ProtocolSource { .. })
                ) {
                    continue;
                }
                let key = TopicKey::new(node_id, port);
                if eval_state.data_bus.is_active(&key) && bufs.1[slot] {
                    static_publish.push((key, last.timestamp, f64::from(bufs.0[slot])));
                }
            }
        }
    }

    drop(filters);

    publish_port_batches(eval_state, &plan_arcs, &all_ws, static_publish);
}
