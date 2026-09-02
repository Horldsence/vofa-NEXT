//! 并发评估执行器 — 图内路径 (评估单元) 分块 fork-join
//!
//! 与串行 [`crate::graph_eval::process_source_batch`] 语义逐项对应, 差异仅在执行拓扑:
//! - **单元→桶**: 编译期评估单元 (见 `node_lower::units`: prelude + 计算连通分量)
//!   按 LPT 贪心装箱为 K 桶 (K = `PipelineConfig.eval_workers`), `std::thread::scope`
//!   每批派生 K-1 个 worker (协调者跑桶 0), 批内分块 (chunk) fork-join
//! - **槽位副本**: 每桶对所涉及的图各持一份槽位缓冲副本; 单元写集互斥 (编译期
//!   切分不变量), 跨单元读只指向 prelude 槽位 — 每份副本先跑 prelude 再跑本桶
//!   计算单元, 副本内自洽
//! - **读路径归桶**: 端口批/派生边/频谱槽按 "正本槽位 → 所属单元 → 桶" 分派到
//!   恰好一个桶; worker 只写私有 staging, join 后协调者按 (桶序→图序→槽位序→帧序)
//!   回放 — 派生环与主时间戳轴的 1:1 对齐、端口批与快照语义与串行版一致
//! - **共享状态零克隆交接**: `source_frames`/`source_texts`/`decoder_states`
//!   `mem::take` 整表取出共享只读 ([`crate::graph_eval::PutBack`] 保证任何退路
//!   原样写回), filter/ifft/trigger 状态按单元 id 表切分为每桶子 map, 批尾合并
//!   写回 (worker panic 亦先合并再续传 — 懒建语义兜底残留缺口)
//! - **静态图**: `is_static_local` 的图每批评估一次 (输入批内不变, 输出值相同);
//!   派生边仍逐帧重复 push 常值保持时间轴对齐, 端口批降为批尾单样本 (常值线视觉不变)
//!
//! 锁序与串行路径一致 (input_values → custom_outputs → source_texts → graphs →
//! filter → decoder → ifft → trigger → analyzers), 批间仍互斥。

use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use buffer_databuffer::DataBuffer;
use dsp_fft::IfftState;
use dsp_filter::DigitalFilter;
use node_engine::{CompiledGraph, SourceFramesMap, SourceTextsMap};
use node_eval::CompiledEval;
use node_frame_decoder::FrameParser;
use node_kind::NodeKind;
use node_trigger::TriggerState;
use pipeline_bus::TopicKey;
use rustc_hash::FxHashMap;
use vofa_core::DataFrame;

use crate::eval_state::GraphEvalState;
use crate::graph_eval::{
    graph_triggered_by, merge_str_map, put_back, EvalBreakdown, PutBack, SlotBufs,
};

/// 分块大小 — 每 chunk 一次 fork-join 屏障 + 快照发布点检查 (对齐串行版
/// "每 1024 帧检查一次" 的节流语义)
const EVAL_CHUNK: usize = 1024;

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
}

/// 单图在桶内的执行计划 — 本桶承担的单元 + 归属本桶的读路径表
struct BucketGraphPlan {
    gi: usize,
    /// 本桶承担的计算单元下标 (不含 prelude; prelude 由所有涉及本图的桶本地复跑)
    unit_ids: Vec<usize>,
    /// (正本槽位, buffer 派生索引) — 正本属本桶
    derived: Vec<(usize, usize)>,
    /// 归属本桶的端口批 (DataBus 批尾发布)
    ports: Vec<PortStage>,
    /// 本图频谱项在 [`BucketPlan::spectra`] 中的下标
    spectra: Vec<usize>,
}

/// 端口批 staging — 每帧 written 置位才追加 (与串行版 port_batches 语义一致)
struct PortStage {
    key: TopicKey,
    slot: usize,
    timestamps: Vec<u64>,
    values: Vec<f64>,
}

/// 每桶执行计划 — 专属槽位副本 + 读路径表 + staging + 状态子 map
struct BucketPlan {
    graphs: Vec<BucketGraphPlan>,
    /// 槽位副本 (gi → 缓冲)
    copies: FxHashMap<usize, SlotBufs>,
    /// (图下标, sink_id, 正本槽位) — 归属本桶的频谱项
    spectra: Vec<(usize, String, Option<usize>)>,
    /// 派生 staging (buffer 派生索引, 值) — 每 chunk 由协调者 drain
    staged_derived: Vec<(usize, f32)>,
    /// 频谱 staging (桶级 spectra 表下标, 值)
    staged_spectra: Vec<(u32, f32)>,
    filters: HashMap<String, DigitalFilter>,
    iffts: HashMap<String, IfftState>,
    triggers: HashMap<String, TriggerState>,
}

impl BucketPlan {
    fn new() -> Self {
        Self {
            graphs: Vec::new(),
            copies: FxHashMap::default(),
            spectra: Vec::new(),
            staged_derived: Vec::new(),
            staged_spectra: Vec::new(),
            filters: HashMap::new(),
            iffts: HashMap::new(),
            triggers: HashMap::new(),
        }
    }

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
    use std::sync::atomic::Ordering;

    if frames.is_empty() {
        return;
    }

    // —— 锁序: 与串行路径一致 (见模块注释)
    let input_values = eval_state.input_values.lock().clone();
    let custom_outputs = eval_state.custom_outputs.lock().clone();
    let mut texts_guard = eval_state.source_texts.lock();
    let texts = put_back(&mut *texts_guard);
    let graphs = eval_state.graphs.lock();
    let graphs_version = eval_state.graphs_version.load(Ordering::Relaxed);
    let mut filters_guard = eval_state.filter_states.lock();
    let mut filters = put_back(&mut *filters_guard);
    let mut decoders_guard = eval_state.decoder_states.lock();
    let decoders = put_back(&mut *decoders_guard);
    let mut iffts_guard = eval_state.ifft_states.lock();
    let mut iffts = put_back(&mut *iffts_guard);
    let mut triggers_guard = eval_state.trigger_states.lock();
    let mut triggers = put_back(&mut *triggers_guard);
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
            &input_values,
            &custom_outputs,
            filters.get_mut(),
            decoders.get(),
            iffts.get_mut(),
            triggers.get_mut(),
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

    // —— 单元→桶 LPT 装箱 (含 prelude 任务; 权重降序 → 最轻桶, 编译/批次确定性)
    let mut tasks: Vec<(u32, usize, usize)> = Vec::new(); // (weight, gi, unit)
    for (gi, g) in graph_list.iter().enumerate() {
        for (u, unit) in g.compiled().units().iter().enumerate() {
            tasks.push((unit.weight, gi, u));
        }
    }
    tasks.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let k = eval_workers.max(1).min(tasks.len().max(1));
    let mut bucket_loads = vec![0u32; k];
    let mut buckets: Vec<BucketPlan> = (0..k).map(|_| BucketPlan::new()).collect();
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
        buckets[b].ensure_graph(gi);
        if unit != 0 {
            buckets[b].graph_mut(gi).unit_ids.push(unit);
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
                buckets[b].ensure_graph(gi);
                buckets[b].graph_mut(gi).ports.push(PortStage {
                    key,
                    slot,
                    timestamps: Vec::with_capacity(frames.len()),
                    values: Vec::with_capacity(frames.len()),
                });
            }
        }
        for (slot, buf_idx) in derived_edges[gi].drain(..) {
            let b = bucket_of(&unit_bucket[gi], slot_unit, slot);
            buckets[b].ensure_graph(gi);
            buckets[b].graph_mut(gi).derived.push((slot, buf_idx));
        }
        for (sink, slot) in compiled.spectrum_slots() {
            // 无上游边: 归 prelude 桶 (永不命中)
            let b = slot.map_or(unit_bucket[gi][0] as usize, |s| {
                bucket_of(&unit_bucket[gi], slot_unit, s)
            });
            buckets[b].ensure_graph(gi);
            buckets[b].spectra.push((gi, sink.clone(), *slot));
            let si = buckets[b].spectra.len() - 1;
            buckets[b].graph_mut(gi).spectra.push(si);
        }
    }

    // 槽位副本预分配 (worker 内零分配; direct 字段访问保持借用切分)
    for bucket in &mut buckets {
        for gp in &bucket.graphs {
            let compiled = graph_list[gp.gi].compiled();
            bucket.copies.insert(gp.gi, new_slot_bufs(compiled));
        }
    }

    // —— 状态切分: 按单元 id 表 drain 出每桶子 map (未认领条目留原表, 批尾合并写回)
    for (gi, g) in graph_list.iter().enumerate() {
        for (u, unit) in g.compiled().units().iter().enumerate().skip(1) {
            let b = unit_bucket[gi][u] as usize;
            let bucket = &mut buckets[b];
            for id in &unit.filter_ids {
                if let Some(v) = filters.get_mut().remove(id.as_ref()) {
                    bucket.filters.insert(id.clone().into_string(), v);
                }
            }
            for id in &unit.ifft_ids {
                if let Some(v) = iffts.get_mut().remove(id.as_ref()) {
                    bucket.iffts.insert(id.clone().into_string(), v);
                }
            }
            for id in &unit.trigger_ids {
                if let Some(v) = triggers.get_mut().remove(id.as_ref()) {
                    bucket.triggers.insert(id.clone().into_string(), v);
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

    let ctx = BatchCtx {
        graph_list: &graph_list,
        frames,
        sf_map: sf_map.get(),
        texts: texts.get(),
        inputs: &input_values,
        customs: &custom_outputs,
        decoders: decoders.get(),
        trigger_src_idx: &trigger_src_idx,
    };

    // —— 分块 fork-join: 每块 push_frame → 并行评估 + staging → join → 按序回放。
    // panic 时先合并桶状态再续传 (PutBack 保证共享表写回; 批次与串行版一致中断)
    let publish_interval = std::time::Duration::from_millis(8);
    let chunk_result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        let mut last_publish = std::time::Instant::now();
        let mut chunk_start = 0;
        while chunk_start < frames.len() {
            let chunk_end = (chunk_start + EVAL_CHUNK).min(frames.len());

            // 1. 本块原始帧进 buffer (派生值 join 后回放, 计数 1:1 对齐)
            let t = std::time::Instant::now();
            for frame in &frames[chunk_start..chunk_end] {
                buffer.push_frame(frame);
            }
            breakdown.push_frame_ns += ns_since(t);

            // 2. 并行评估 + staging (worker panic 经 scope 传播到本闭包);
            //    无动态图 (纯静态/无触发) 时仅静态图路径, 跳过 worker 分派
            let t = std::time::Instant::now();
            if !graph_list.is_empty() {
                let (first, rest) = buckets.split_at_mut(1);
                let ctx_ref = &ctx;
                std::thread::scope(|scope| {
                    for bucket in rest.iter_mut() {
                        scope.spawn(move || {
                            run_bucket_chunk(bucket, ctx_ref, chunk_start..chunk_end);
                        });
                    }
                    run_bucket_chunk(&mut first[0], &ctx, chunk_start..chunk_end);
                });
            }
            breakdown.graph_eval_ns += ns_since(t);

            // 3. 派生回放 (桶序 → 图序 → 槽位序 → 帧序; 各派生环内部保持帧序)
            let t = std::time::Instant::now();
            for bucket in &mut buckets {
                for (buf_idx, value) in bucket.staged_derived.drain(..) {
                    buffer.push_derived_idx(buf_idx, value);
                }
            }
            // 静态图逐帧重复 push 常值 (written 常量 — 与串行版逐帧评估后的回放等价)
            for _ in chunk_start..chunk_end {
                for (gi, edges) in static_edges.iter().enumerate() {
                    let bufs = &static_bufs[gi];
                    for (slot, buf_idx) in edges {
                        if bufs.1[*slot] {
                            buffer.push_derived_idx(*buf_idx, bufs.0[*slot]);
                        }
                    }
                }
            }
            breakdown.derived_ns += ns_since(t);

            // 4. 频谱回放 (各 sink 内部保持帧序)
            let t = std::time::Instant::now();
            for bucket in &mut buckets {
                for (si, value) in bucket.staged_spectra.drain(..) {
                    let (_, sink, _) = &bucket.spectra[si as usize];
                    if let Some(analyzer) = analyzers.get_mut(sink) {
                        analyzer.push(value);
                    }
                }
            }
            breakdown.spectrum_ns += ns_since(t);

            // 5. 批内节流快照发布 (每 chunk 边界检查, ≥8ms 物化)
            if last_publish.elapsed() >= publish_interval {
                publish_str(
                    &graph_list,
                    &buckets,
                    &unit_bucket,
                    &static_list,
                    &static_bufs,
                    &mut eval_state.graph_string_outputs.lock(),
                );
                let mut snap = eval_state.output_snapshot.lock();
                publish_snapshot_values(
                    &graph_list,
                    &buckets,
                    &unit_bucket,
                    &static_list,
                    &static_bufs,
                    &mut snap.values,
                );
                snap.tick = snap.tick.wrapping_add(1);
                drop(snap);
                last_publish = std::time::Instant::now();
            }

            chunk_start = chunk_end;
        }
    }));

    // 桶状态合并写回 (panic 路径也执行, 之后按需续传)
    for bucket in &mut buckets {
        filters.get_mut().extend(bucket.filters.drain());
        iffts.get_mut().extend(bucket.iffts.drain());
        triggers.get_mut().extend(bucket.triggers.drain());
    }
    if let Err(panic_payload) = chunk_result {
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
            &buckets,
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
        &buckets,
        &unit_bucket,
        &static_list,
        &static_bufs,
        &mut eval_state.graph_string_outputs.lock(),
    );

    drop(analyzers);
    drop(triggers);
    drop(iffts);
    drop(decoders);
    drop(filters);
    drop(texts);

    // 触发源最新帧写回缓存 (latest-value 融合 — 与串行版逐帧覆盖的批尾效果一致)
    let last = frames.last().expect("frames 非空已检查");
    match sf_map.get_mut().get_mut(source_id) {
        Some(slot) => {
            slot.timestamp = last.timestamp;
            slot.channels.clone_from(&last.channels);
        }
        None => {
            sf_map.get_mut().insert(source_id.to_string(), last.clone());
        }
    }

    // —— DataBus 端口批发布 (桶序 → 图序 → 槽位序; 静态图批尾单样本)
    for bucket in &buckets {
        for gp in &bucket.graphs {
            for pb in &gp.ports {
                if !pb.values.is_empty() {
                    eval_state.data_bus.publish_samples(
                        pb.key.clone(),
                        Arc::from(&pb.timestamps[..]),
                        Arc::from(&pb.values[..]),
                    );
                }
            }
        }
    }
    let last_ts = last.timestamp;
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
                eval_state.data_bus.publish_samples(
                    key,
                    Arc::from([last_ts]),
                    Arc::from([f64::from(bufs.0[slot])]),
                );
            }
        }
    }
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

/// 单桶单块执行 — 逐帧: prelude → 本桶计算单元 → staging (仅 written 置位)
fn run_bucket_chunk(bucket: &mut BucketPlan, ctx: &BatchCtx<'_>, range: std::ops::Range<usize>) {
    for frame_i in range {
        let frame = &ctx.frames[frame_i];
        for gp in &mut bucket.graphs {
            let g = ctx.graph_list[gp.gi];
            let compiled = g.compiled();
            let copy = bucket
                .copies
                .get_mut(&gp.gi)
                .expect("槽位副本已在批首预分配");
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
                &mut bucket.filters,
                ctx.decoders,
                &mut bucket.iffts,
                &mut bucket.triggers,
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
                    &mut bucket.filters,
                    ctx.decoders,
                    &mut bucket.iffts,
                    &mut bucket.triggers,
                    &mut copy.0,
                    &mut copy.1,
                    &mut copy.2,
                    &mut copy.3,
                );
            }
            for (slot, buf_idx) in &gp.derived {
                if copy.1[*slot] {
                    bucket.staged_derived.push((*buf_idx, copy.0[*slot]));
                }
            }
            for pb in &mut gp.ports {
                if copy.1[pb.slot] {
                    pb.timestamps.push(frame.timestamp);
                    pb.values.push(f64::from(copy.0[pb.slot]));
                }
            }
            for &si in &gp.spectra {
                if let Some(slot) = bucket.spectra[si].2 {
                    if copy.1[slot] {
                        bucket
                            .staged_spectra
                            .push((u32::try_from(si).unwrap_or(u32::MAX), copy.0[slot]));
                    }
                }
            }
        }
    }
}

/// 物化所有图当前值进快照 values (动态图按单元→桶副本, 静态图整表)
fn publish_snapshot_values(
    graph_list: &[&CompiledGraph],
    buckets: &[BucketPlan],
    unit_bucket: &[Vec<u32>],
    static_list: &[&CompiledGraph],
    static_bufs: &[SlotBufs],
    out: &mut node_engine::ValuesMap,
) {
    for (gi, g) in graph_list.iter().enumerate() {
        let compiled = g.compiled();
        for (u, unit) in compiled.units().iter().enumerate() {
            let b = unit_bucket[gi][u] as usize;
            let Some(copy) = buckets[b].copies.get(&gi) else {
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
    buckets: &[BucketPlan],
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
            let Some(copy) = buckets[b].copies.get(&gi) else {
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

fn ns_since(t: std::time::Instant) -> u64 {
    u64::try_from(t.elapsed().as_nanos()).unwrap_or(u64::MAX)
}
