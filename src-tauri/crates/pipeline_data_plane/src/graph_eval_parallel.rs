//! 并发评估执行器 — 图内路径 (评估单元) 分块 fork-join
//!
//! 与串行 [`crate::graph_eval::process_source_batch`] 语义逐项对应, 差异仅在执行拓扑:
//! - **单元→桶**: 编译期评估单元 (见 `node_lower::units`: prelude + 计算连通分量)
//!   按 LPT 贪心装箱为 K 桶 (K = `PipelineConfig.eval_workers`); 批内 spawn K-1 个
//!   常驻 worker (协调者跑桶 0), 以双屏障 (评估/回放) 分块推进 — 避免 per-chunk
//!   重新 spawn 线程的固定开销
//! - **槽位副本**: 每桶对所涉及的图各持一份槽位缓冲副本; 单元写集互斥 (编译期
//!   切分不变量), 跨单元读只指向 prelude 槽位 — 每份副本先跑 prelude 再跑本桶
//!   计算单元, 副本内自洽
//! - **读路径归桶**: 端口批/派生边/频谱槽按 "正本槽位 → 所属单元 → 桶" 分派到
//!   恰好一个桶; worker 只写私有 staging, 每块结束经 stage 交换槽交给协调者,
//!   按确定序回放 — 派生环与主时间戳轴的 1:1 对齐、端口批与快照语义与串行版一致
//! - **快照**: 中途发布点协调者置发布标志, worker 在块尾把本桶单元的物化增量
//!   随 staging 交换带回, 协调者合并进 snap.values — 节奏与串行版一致
//! - **共享状态零克隆交接**: `source_frames`/`source_texts`/`decoder_states`
//!   `mem::take` 整表取出共享只读 ([`crate::graph_eval::PutBack`] 保证任何退路
//!   原样写回); filter/ifft/trigger 状态按单元 id 表切分为每桶子 map, 批尾合并
//!   写回 (worker panic 时该桶子状态丢失 — 懒建语义兜底; 批次与串行版一致中断)
//! - **静态图**: `is_static_local` 的图每批评估一次 (输入批内不变, 输出值相同);
//!   派生边仍逐帧重复 push 常值保持时间轴对齐, 端口批降为批尾单样本 (常值线视觉不变)
//!
//! 锁序与串行路径一致 (input_values → custom_outputs → source_texts → graphs →
//! filter → decoder → ifft → trigger → analyzers), 批间仍互斥。

use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use buffer_databuffer::DataBuffer;
use dsp_fft::{IfftState, SpectrumAnalyzer};
use dsp_filter::DigitalFilter;
use node_engine::{CompiledGraph, SourceFramesMap, SourceTextsMap};
use node_eval::CompiledEval;
use node_frame_decoder::FrameParser;
use node_kind::NodeKind;
use node_trigger::TriggerState;
use parking_lot::Mutex;
use pipeline_bus::TopicKey;
use rustc_hash::FxHashMap;
use vofa_core::DataFrame;

use crate::eval_state::GraphEvalState;
use crate::graph_eval::{
    graph_triggered_by, merge_str_map, EvalBreakdown, PutBack, SlotBufs, TakeGuard,
};

/// 分块大小 — 每块一次屏障对; 高帧率下 8ms 快照节流不受影响 (4096 帧 @700k
/// fps ≈ 5.8ms < 8ms 门限), 低帧率批次天然单块
const EVAL_CHUNK: usize = 4096;

/// 共享只读批上下文 — worker 跨线程借用
struct BatchCtx<'a> {
    graph_list: &'a [&'a CompiledGraph],
    frames: &'a [DataFrame],
    sf_map: &'a SourceFramesMap,
    texts: &'a SourceTextsMap,
    inputs: &'a HashMap<String, f32>,
    customs: &'a HashMap<String, HashMap<String, f32>>,
    decoders: &'a HashMap<String, FrameParser>,
    /// 每图触发源在 frame_sources 中的下标 (None = 不引用该源, 不做帧覆盖)
    trigger_src_idx: &'a [Option<usize>],
    /// 中途快照发布标志 (协调者置位, worker 块尾采样)
    publish_due: &'a AtomicBool,
}

/// 单图在桶内的执行计划 (批内只读)
struct BucketGraphPlan {
    gi: usize,
    /// 本桶承担的计算单元下标 (不含 prelude; prelude 由所有涉及本图的桶本地复跑)
    unit_ids: Vec<usize>,
    /// (正本槽位, buffer 派生索引) — 正本属本桶
    derived: Vec<(usize, usize)>,
    /// 端口批路由下标 (桶级扁平表 [`BucketPlan::port_routes`])
    ports: Vec<usize>,
    /// 本图频谱项在 [`BucketPlan::spectra`] 中的下标
    spectra: Vec<usize>,
}

/// 桶执行计划 — 只读路由表 (Arc 共享给 worker)
struct BucketPlan {
    graphs: Vec<BucketGraphPlan>,
    /// (图下标, sink_id, 正本槽位) — 归属本桶的频谱项
    spectra: Vec<(usize, String, Option<usize>)>,
    /// 端口批路由: (topic, 正本槽位)
    port_routes: Vec<(TopicKey, usize)>,
}

/// worker 私有可变状态 — 批尾经归还箱交回协调者
struct WorkerState {
    copies: FxHashMap<usize, SlotBufs>,
    filters: HashMap<String, DigitalFilter>,
    iffts: HashMap<String, IfftState>,
    triggers: HashMap<String, TriggerState>,
    /// 与 [`BucketPlan::port_routes`] 按下标对齐
    ports: Vec<PortAccum>,
    /// 派生 staging (buffer 派生索引, 值) — 每块与协调者交换
    staged_derived: Vec<(usize, f32)>,
    /// 频谱 staging (桶级 spectra 表下标, 值)
    staged_spectra: Vec<(u32, f32)>,
    /// 快照物化增量 (发布标志置位时块尾物化, 随 staging 交换带回)
    snapshot_delta: Option<(node_engine::ValuesMap, node_engine::StringValuesMap)>,
}

/// staging 交换槽 — worker 与协调者经互斥锁成对交接
struct StageSlot {
    staged_derived: Vec<(usize, f32)>,
    staged_spectra: Vec<(u32, f32)>,
    snapshot_delta: Option<(node_engine::ValuesMap, node_engine::StringValuesMap)>,
}

impl StageSlot {
    const fn new() -> Self {
        Self {
            staged_derived: Vec::new(),
            staged_spectra: Vec::new(),
            snapshot_delta: None,
        }
    }

    /// 与 worker 私有 staging 原位交换 (worker 拿回旧缓冲复用, 协调者取得本块产物)
    const fn swap_from(&mut self, ws: &mut WorkerState) {
        std::mem::swap(&mut self.staged_derived, &mut ws.staged_derived);
        std::mem::swap(&mut self.staged_spectra, &mut ws.staged_spectra);
        std::mem::swap(&mut self.snapshot_delta, &mut ws.snapshot_delta);
    }
}

/// 端口批累积 — 每帧 written 置位才追加 (与串行版 port_batches 语义一致)
struct PortAccum {
    route: usize,
    slot: usize,
    timestamps: Vec<u64>,
    values: Vec<f64>,
}

/// 并发批处理入口 — `eval_workers ≥ 2` 时由 `on_frames_detached` 调入
///
/// `buffer` / `source_frames` 已由调用方持锁 (guard 解引用传递)。
pub(crate) fn process_source_batch_parallel(
    eval_state: &GraphEvalState,
    source_frames: &mut SourceFramesMap,
    source_id: &str,
    frames: &[DataFrame],
    buffer: &mut DataBuffer,
    eval_workers: usize,
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
        return; // put_back 守卫落栈时原样写回
    }

    // —— 静态图: 每批评估一次 (输入批内不变; 无状态节点, 全表传参无副作用)
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
        for e in g.edges() {
            if let Some(slot) = g.compiled().slot_of(&e.source, &e.source_handle) {
                edges.push((slot, buffer.derived_index_of(&e.target, &e.source)));
            }
        }
    }

    // —— 动态图派生边预计算: (正本槽位, buffer 派生索引), 读路径归桶在下面分派
    let mut derived_edges: Vec<Vec<(usize, usize)>> = vec![Vec::new(); graph_list.len()];
    for (gi, g) in graph_list.iter().enumerate() {
        for e in g.edges() {
            if let Some(slot) = g.compiled().slot_of(&e.source, &e.source_handle) {
                derived_edges[gi].push((slot, buffer.derived_index_of(&e.target, &e.source)));
            }
        }
    }

    // —— 单元→桶 LPT 装箱 (含 prelude 任务; 权重降序 → 最轻桶, 批次确定性)
    let mut tasks: Vec<(u32, usize, usize)> = Vec::new(); // (weight, gi, unit)
    for (gi, g) in graph_list.iter().enumerate() {
        for (u, unit) in g.compiled().units().iter().enumerate() {
            tasks.push((unit.weight, gi, u));
        }
    }
    tasks.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let k = eval_workers.max(1).min(tasks.len().max(1));
    let mut bucket_loads = vec![0u32; k];
    let mut plans: Vec<BucketPlan> = (0..k)
        .map(|_| BucketPlan {
            graphs: Vec::new(),
            spectra: Vec::new(),
            port_routes: Vec::new(),
        })
        .collect();
    let mut unit_bucket: Vec<Vec<u32>> = graph_list
        .iter()
        .map(|g| vec![0u32; g.compiled().units().len()])
        .collect();
    for (weight, gi, unit) in tasks {
        let b = bucket_loads
            .iter()
            .enumerate()
            .min_by_key(|(_, load)| **load)
            .map_or(0, |(i, _)| i);
        bucket_loads[b] += weight;
        unit_bucket[gi][unit] = u32::try_from(b).unwrap_or(u32::MAX);
        plans[b].ensure_graph(gi);
        if unit != 0 {
            plans[b].graph_mut(gi).unit_ids.push(unit);
        }
    }

    // —— 读路径归桶: 正本槽位 → 所属单元 → 桶 (端口批 / 派生边 / 频谱项)
    for (gi, g) in graph_list.iter().enumerate() {
        let compiled = g.compiled();
        let slot_unit = compiled.slot_unit();
        for (slot, (node_id, port)) in compiled.slot_names().iter().enumerate() {
            // ProtocolSource 已由帧分发按源发布 (与串行版一致整节点跳过)
            if matches!(
                g.value_def(node_id).map(|node| &node.kind),
                Some(NodeKind::ProtocolSource { .. })
            ) {
                continue;
            }
            let key = TopicKey::new(node_id, port);
            if eval_state.data_bus.is_active(&key) {
                let b = bucket_of(&unit_bucket[gi], slot_unit, slot);
                plans[b].ensure_graph(gi);
                plans[b].port_routes.push((key, slot));
                let route = plans[b].port_routes.len() - 1;
                plans[b].graph_mut(gi).ports.push(route);
            }
        }
        for (slot, buf_idx) in derived_edges[gi].drain(..) {
            let b = bucket_of(&unit_bucket[gi], slot_unit, slot);
            plans[b].ensure_graph(gi);
            plans[b].graph_mut(gi).derived.push((slot, buf_idx));
        }
        for (sink, slot) in compiled.spectrum_slots() {
            // 无上游边: 归 prelude 桶 (永不命中)
            let b = slot.map_or(unit_bucket[gi][0] as usize, |s| {
                bucket_of(&unit_bucket[gi], slot_unit, s)
            });
            plans[b].ensure_graph(gi);
            plans[b].spectra.push((gi, sink.clone(), *slot));
            let si = plans[b].spectra.len() - 1;
            plans[b].graph_mut(gi).spectra.push(si);
        }
    }

    // —— 状态切分: 按单元 id 表 drain 出每桶子 map (未认领条目留原表, 批尾合并写回)
    let plan_arcs: Vec<Arc<BucketPlan>> = plans.into_iter().map(Arc::new).collect();
    let mut worker_states: Vec<WorkerState> = plan_arcs
        .iter()
        .map(|plan| {
            let mut ws = WorkerState::new(plan);
            for gp in &plan.graphs {
                ws.copies
                    .insert(gp.gi, new_slot_bufs(graph_list[gp.gi].compiled()));
            }
            ws
        })
        .collect();
    for (b, plan) in plan_arcs.iter().enumerate() {
        let ws = &mut worker_states[b];
        for gp in &plan.graphs {
            let units = graph_list[gp.gi].compiled().units();
            // 仅认领本桶承担单元的状态 (prelude 无状态; 单元写集互斥 → 无跨桶争用)
            for &u in &gp.unit_ids {
                let unit = &units[u];
                for id in &unit.filter_ids {
                    if let Some(v) = filters_all.remove(id.as_ref()) {
                        ws.filters.insert(id.clone().into_string(), v);
                    }
                }
                for id in &unit.ifft_ids {
                    if let Some(v) = iffts_all.remove(id.as_ref()) {
                        ws.iffts.insert(id.clone().into_string(), v);
                    }
                }
                for id in &unit.trigger_ids {
                    if let Some(v) = triggers_all.remove(id.as_ref()) {
                        ws.triggers.insert(id.clone().into_string(), v);
                    }
                }
            }
        }
    }

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
    let return_slots: Vec<Arc<Mutex<Option<WorkerState>>>> =
        (1..k).map(|_| Arc::new(Mutex::new(None))).collect();
    let swap_slots: Vec<Arc<Mutex<StageSlot>>> = (1..k)
        .map(|_| Arc::new(Mutex::new(StageSlot::new())))
        .collect();
    let mut last_publish = std::time::Instant::now();
    // std Barrier 无中毒机制: worker panic 时置位 broken (避免其余线程屏障死锁),
    // panic payload 经 panic_slot 传递, 批尾统一续传 (保持与串行版一致的批次中断)
    let broken = AtomicBool::new(false);
    let panic_slot: Arc<Mutex<Option<Box<dyn std::any::Any + Send>>>> = Arc::new(Mutex::new(None));

    let chunk_result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        if k == 1 {
            // 单桶: 协调者直跑 (staging 私有直 drain, 无屏障)
            for &(cs, ce) in &chunks {
                let t = std::time::Instant::now();
                for frame in &frames[cs..ce] {
                    buffer.push_frame(frame);
                }
                breakdown.push_frame_ns += ns_since(t);

                let t = std::time::Instant::now();
                run_bucket_chunk(&lead_plan, &mut lead, &ctx, (cs, ce));
                breakdown.graph_eval_ns += ns_since(t);

                let t = std::time::Instant::now();
                drain_worker(&lead_plan, &mut lead, buffer, &mut analyzers, breakdown);
                push_static_derived(&static_edges, &static_bufs, (cs, ce), buffer);
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
            let eval_barrier = Arc::new(SpinBarrier::new(k));
            let drain_barrier = Arc::new(SpinBarrier::new(k));
            let chunks_ref = &chunks;
            let ctx_ref = &ctx;
            std::thread::scope(|scope| {
                for b in 1..k {
                    let plan = Arc::clone(&plan_arcs[b]);
                    let swap_slot = Arc::clone(&swap_slots[b - 1]);
                    let slot_handle = Arc::clone(&return_slots[b - 1]);
                    let eval_barrier = Arc::clone(&eval_barrier);
                    let drain_barrier = Arc::clone(&drain_barrier);
                    let ws = states_iter.next().expect("worker 状态已预留");
                    let broken = &broken;
                    let panic_slot = Arc::clone(&panic_slot);
                    scope.spawn(move || {
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
                                ws.snapshot_delta = Some(materialize_bucket(&plan, &ws, ctx_ref));
                            }
                            {
                                let mut g = swap_slot.lock();
                                g.swap_from(&mut ws);
                            }
                            eval_barrier.wait();
                            drain_barrier.wait();
                            if broken.load(Ordering::Relaxed) {
                                break;
                            }
                        }
                        *slot_handle.lock() = Some(ws);
                    });
                }

                // 协调者: 桶 0 执行 + 每块屏障间回放/发布
                for &(cs, ce) in chunks_ref {
                    let t = std::time::Instant::now();
                    for frame in &frames[cs..ce] {
                        buffer.push_frame(frame);
                    }
                    breakdown.push_frame_ns += ns_since(t);

                    let t = std::time::Instant::now();
                    run_bucket_chunk(&lead_plan, &mut lead, &ctx, (cs, ce));
                    breakdown.graph_eval_ns += ns_since(t);

                    eval_barrier.wait();
                    if broken.load(Ordering::Relaxed) {
                        break;
                    }

                    let t = std::time::Instant::now();
                    drain_worker(&lead_plan, &mut lead, buffer, &mut analyzers, breakdown);
                    for (b, slot) in swap_slots.iter().enumerate() {
                        let mut g = slot.lock();
                        for (buf_idx, value) in g.staged_derived.drain(..) {
                            buffer.push_derived_idx(buf_idx, value);
                        }
                        for (si, value) in g.staged_spectra.drain(..) {
                            let (_, sink, _) = &plan_arcs[b + 1].spectra[si as usize];
                            if let Some(analyzer) = analyzers.get_mut(sink) {
                                analyzer.push(value);
                            }
                        }
                    }
                    push_static_derived(&static_edges, &static_bufs, (cs, ce), buffer);
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

                    drain_barrier.wait();
                    if broken.load(Ordering::Relaxed) {
                        break;
                    }
                }
            });
        }
    }));

    drop(chunk_result); // Err payload 已由 panic_slot 传递并续传

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
    let panic_payload = panic_slot.lock().take();
    if let Some(panic_payload) = panic_payload {
        std::panic::resume_unwind(panic_payload);
    }

    // —— 批尾最终发布 (与串行版同语义: 版本变化先清空, 保证过期键不回流)
    let version_changed = {
        let mut snap = eval_state.output_snapshot.lock();
        let changed = snap.graphs_version != graphs_version;
        if changed {
            snap.values.clear();
            snap.graphs_version = graphs_version;
        }
        publish_snapshot_values(
            &graph_list,
            &all_ws,
            &unit_bucket,
            &static_list,
            &static_bufs,
            &mut snap.values,
        );
        snap.tick = snap.tick.wrapping_add(1);
        changed
    };
    if version_changed {
        eval_state.graph_string_outputs.lock().clear();
    }
    publish_str(
        &graph_list,
        &all_ws,
        &unit_bucket,
        &static_list,
        &static_bufs,
        &mut eval_state.graph_string_outputs.lock(),
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

    // —— DataBus 端口批发布 (桶序 → 路由序; 静态图批尾单样本, 锁内收集锁外发布)
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

    for (b, ws) in all_ws.iter().enumerate() {
        for acc in &ws.ports {
            if !acc.values.is_empty() {
                let (key, _) = &plan_arcs[b].port_routes[acc.route];
                eval_state.data_bus.publish_samples(
                    key.clone(),
                    Arc::from(&acc.timestamps[..]),
                    Arc::from(&acc.values[..]),
                );
            }
        }
    }
    for (key, ts, value) in static_publish {
        eval_state
            .data_bus
            .publish_samples(key, Arc::from([ts]), Arc::from([value]));
    }
}

/// 单桶单块执行 — 逐帧: prelude → 本桶计算单元 → staging (仅 written 置位)
fn run_bucket_chunk(
    plan: &BucketPlan,
    ws: &mut WorkerState,
    ctx: &BatchCtx<'_>,
    chunk: (usize, usize),
) {
    for frame_i in chunk.0..chunk.1 {
        let frame = &ctx.frames[frame_i];
        for gp in &plan.graphs {
            let g = ctx.graph_list[gp.gi];
            let compiled = g.compiled();
            let copy = ws.copies.get_mut(&gp.gi).expect("槽位副本已在批首预分配");
            let resolved = compiled.resolve_frames(
                ctx.sf_map,
                ctx.trigger_src_idx[gp.gi].map(|idx| (idx, frame)),
            );
            let units = compiled.units();
            // prelude 先行 (供给槽位写本副本, 计算单元跨单元读由此满足)
            compiled.run_unit_frame(
                &units[0],
                resolved.as_slice(),
                ctx.texts,
                ctx.inputs,
                ctx.customs,
                &mut ws.filters,
                ctx.decoders,
                &mut ws.iffts,
                &mut ws.triggers,
                &mut copy.0,
                &mut copy.1,
                &mut copy.2,
                &mut copy.3,
            );
            for &u in &gp.unit_ids {
                compiled.run_unit_frame(
                    &units[u],
                    resolved.as_slice(),
                    ctx.texts,
                    ctx.inputs,
                    ctx.customs,
                    &mut ws.filters,
                    ctx.decoders,
                    &mut ws.iffts,
                    &mut ws.triggers,
                    &mut copy.0,
                    &mut copy.1,
                    &mut copy.2,
                    &mut copy.3,
                );
            }
            for (slot, buf_idx) in &gp.derived {
                if copy.1[*slot] {
                    ws.staged_derived.push((*buf_idx, copy.0[*slot]));
                }
            }
            for route in &gp.ports {
                let acc = &mut ws.ports[*route];
                let slot = acc.slot;
                if copy.1[slot] {
                    acc.timestamps.push(frame.timestamp);
                    acc.values.push(f64::from(copy.0[slot]));
                }
            }
            for &si in &gp.spectra {
                if let Some(slot) = plan.spectra[si].2 {
                    if copy.1[slot] {
                        ws.staged_spectra
                            .push((u32::try_from(si).unwrap_or(u32::MAX), copy.0[slot]));
                    }
                }
            }
        }
    }
}

/// 回放单桶 staging — 派生 → buffer; 频谱 → analyzer (块间由屏障定序)
fn drain_worker(
    plan: &BucketPlan,
    ws: &mut WorkerState,
    buffer: &mut DataBuffer,
    analyzers: &mut HashMap<String, SpectrumAnalyzer>,
    breakdown: &mut EvalBreakdown,
) {
    let t = std::time::Instant::now();
    for (buf_idx, value) in ws.staged_derived.drain(..) {
        buffer.push_derived_idx(buf_idx, value);
    }
    breakdown.derived_ns += ns_since(t);
    let t = std::time::Instant::now();
    for (si, value) in ws.staged_spectra.drain(..) {
        let (_, sink, _) = &plan.spectra[si as usize];
        if let Some(analyzer) = analyzers.get_mut(sink) {
            analyzer.push(value);
        }
    }
    breakdown.spectrum_ns += ns_since(t);
}

/// worker 块尾物化本桶单元 → 快照增量 (发布标志置位时调用)
fn materialize_bucket(
    plan: &BucketPlan,
    ws: &WorkerState,
    ctx: &BatchCtx<'_>,
) -> (node_engine::ValuesMap, node_engine::StringValuesMap) {
    let mut values = node_engine::ValuesMap::default();
    let mut str_values = node_engine::StringValuesMap::default();
    for gp in &plan.graphs {
        let compiled = ctx.graph_list[gp.gi].compiled();
        let copy = ws.copies.get(&gp.gi).expect("槽位副本已预分配");
        for u in std::iter::once(0).chain(gp.unit_ids.iter().copied()) {
            let unit = &compiled.units()[u];
            compiled.materialize_unit(unit, &copy.0, &copy.1, &mut values);
            compiled.materialize_str_unit(unit, &copy.2, &copy.3, &mut str_values);
        }
    }
    (values, str_values)
}

/// 静态图派生值逐帧重复 push 常值 (输出批内不变, 且保持派生环与主时间戳轴
/// 的 push 计数 1:1 对齐)
fn push_static_derived(
    static_edges: &[Vec<(usize, usize)>],
    static_bufs: &[SlotBufs],
    chunk: (usize, usize),
    buffer: &mut DataBuffer,
) {
    for _ in chunk.0..chunk.1 {
        for (gi, edges) in static_edges.iter().enumerate() {
            let bufs = &static_bufs[gi];
            for (slot, buf_idx) in edges {
                if bufs.1[*slot] {
                    buffer.push_derived_idx(*buf_idx, bufs.0[*slot]);
                }
            }
        }
    }
}

/// 快照发布点 — 物化全部图 + 字符串输出 (节流由调用方判定)
fn publish_point(
    eval_state: &GraphEvalState,
    graph_list: &[&CompiledGraph],
    worker_states: &[WorkerState],
    unit_bucket: &[Vec<u32>],
    static_list: &[&CompiledGraph],
    static_bufs: &[SlotBufs],
) {
    let mut snap = eval_state.output_snapshot.lock();
    publish_snapshot_values(
        graph_list,
        worker_states,
        unit_bucket,
        static_list,
        static_bufs,
        &mut snap.values,
    );
    snap.tick = snap.tick.wrapping_add(1);
    drop(snap);
    publish_str(
        graph_list,
        worker_states,
        unit_bucket,
        static_list,
        static_bufs,
        &mut eval_state.graph_string_outputs.lock(),
    );
}

/// 物化所有图当前值进快照 values (动态图按单元→桶副本, 静态图整表)
fn publish_snapshot_values(
    graph_list: &[&CompiledGraph],
    worker_states: &[WorkerState],
    unit_bucket: &[Vec<u32>],
    static_list: &[&CompiledGraph],
    static_bufs: &[SlotBufs],
    out: &mut node_engine::ValuesMap,
) {
    for (gi, g) in graph_list.iter().enumerate() {
        let compiled = g.compiled();
        for (u, unit) in compiled.units().iter().enumerate() {
            let b = unit_bucket[gi][u] as usize;
            let Some(copy) = worker_states[b].copies.get(&gi) else {
                continue;
            };
            compiled.materialize_unit(unit, &copy.0, &copy.1, out);
        }
    }
    for (bufs, g) in static_bufs.iter().zip(static_list) {
        g.compiled().materialize(&bufs.0, &bufs.1, out);
    }
}

/// 字符串输出发布 (节流点/批尾共用; 语义同串行版 publish_str_slots)
fn publish_str(
    graph_list: &[&CompiledGraph],
    worker_states: &[WorkerState],
    unit_bucket: &[Vec<u32>],
    static_list: &[&CompiledGraph],
    static_bufs: &[SlotBufs],
    out: &mut HashMap<String, HashMap<String, String>>,
) {
    let mut buf = node_engine::StringValuesMap::default();
    for (gi, g) in graph_list.iter().enumerate() {
        let compiled = g.compiled();
        for (u, unit) in compiled.units().iter().enumerate() {
            let b = unit_bucket[gi][u] as usize;
            let Some(copy) = worker_states[b].copies.get(&gi) else {
                continue;
            };
            compiled.materialize_str_unit(unit, &copy.2, &copy.3, &mut buf);
        }
    }
    for (bufs, g) in static_bufs.iter().zip(static_list) {
        g.compiled().materialize_str(&bufs.2, &bufs.3, &mut buf);
    }
    merge_str_map(buf, out);
}

/// 正本槽位 → 桶 (槽位 → 单元 → 桶)
fn bucket_of(unit_bucket: &[u32], slot_unit: &[u32], slot: usize) -> usize {
    unit_bucket[slot_unit[slot] as usize] as usize
}

fn new_slot_bufs(compiled: &CompiledEval) -> SlotBufs {
    (
        vec![0.0; compiled.slot_count()],
        vec![false; compiled.slot_count()],
        vec![String::new(); compiled.str_slot_count()],
        vec![false; compiled.str_slot_count()],
    )
}

fn ns_since(t: std::time::Instant) -> u64 {
    u64::try_from(t.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

/// 自旋屏障 — 分块工作在微秒~百微秒量级, `std::sync::Barrier` 的
/// 互斥锁/条件变量睡眠唤醒 (~50µs) 会吃掉并行收益; 这里先自旋后让出
struct SpinBarrier {
    threads: usize,
    count: AtomicUsize,
    generation: AtomicUsize,
}

impl SpinBarrier {
    const fn new(threads: usize) -> Self {
        Self {
            threads,
            count: AtomicUsize::new(0),
            generation: AtomicUsize::new(0),
        }
    }

    fn wait(&self) {
        let arrived = self.count.fetch_add(1, Ordering::AcqRel) + 1;
        if arrived == self.threads {
            // 最后到达者: 开启新代次放行全体
            self.count.store(0, Ordering::Release);
            self.generation.fetch_add(1, Ordering::Release);
            return;
        }
        let gen = self.generation.load(Ordering::Acquire);
        let mut spins = 0usize;
        while self.generation.load(Ordering::Acquire) == gen {
            if spins < 8_192 {
                std::hint::spin_loop();
            } else {
                std::thread::yield_now();
            }
            spins = spins.wrapping_add(1);
        }
    }
}

impl BucketPlan {
    fn graph_mut(&mut self, gi: usize) -> &mut BucketGraphPlan {
        self.graphs
            .iter_mut()
            .find(|g| g.gi == gi)
            .expect("桶图计划已登记")
    }

    fn ensure_graph(&mut self, gi: usize) {
        if !self.graphs.iter().any(|g| g.gi == gi) {
            self.graphs.push(BucketGraphPlan {
                gi,
                unit_ids: Vec::new(),
                derived: Vec::new(),
                ports: Vec::new(),
                spectra: Vec::new(),
            });
        }
    }
}

impl WorkerState {
    fn new(plan: &BucketPlan) -> Self {
        Self {
            copies: FxHashMap::default(),
            filters: HashMap::new(),
            iffts: HashMap::new(),
            triggers: HashMap::new(),
            ports: plan
                .port_routes
                .iter()
                .enumerate()
                .map(|(route, (_, slot))| PortAccum {
                    route,
                    slot: *slot,
                    timestamps: Vec::new(),
                    values: Vec::new(),
                })
                .collect(),
            staged_derived: Vec::new(),
            staged_spectra: Vec::new(),
            snapshot_delta: None,
        }
    }
}
