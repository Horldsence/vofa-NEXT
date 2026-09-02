//! GPU 批评估 — 无状态 Math 单元的 wgpu 卸载 (图内路径第三执行拓扑)
//!
//! 与串行/并行路径语义逐项对应 (等价测试仲裁), 差异仅在执行拓扑:
//! - **单桶计划**: [`build_plans`] 以 K=1 装箱 — 全部读路径 (端口批/派生边/频谱)
//!   归一个桶, 批尾发布与并行版共用;
//! - **每块三段**:
//!   1. CPU 段 (逐帧): prelude → 本图槽位副本 → CPU-only 单元 → 副本;
//!      CPU 槽位读路径 staging; GPU 单元的 prelude 供给槽位 → 上传矩阵;
//!   2. GPU 段 (每图一次 enqueue, 全图单次 submit): 资格单元 dispatch (一帧一线程);
//!   3. 回读段 (逐帧): 输出矩阵 → 副本 (written 置位 — Math 每帧恒写) →
//!      GPU 槽位读路径 staging;
//! - **失败契约**: 计划/会话构建失败 → 禁用至下个图版本, 本批未推帧直接回退 CPU;
//!   批中失败 → 回滚本块 staging 后全部单元 CPU 续跑, 不丢帧不重复推帧;
//! - **数值**: 纯 ALU 位级一致; ÷/超越函数 ≤2.5 ulp (见 `node_gpu` 契约)。

use std::collections::{BTreeSet, HashMap};
use std::panic::AssertUnwindSafe;
use std::sync::atomic::Ordering;
use std::sync::Arc;


use buffer_databuffer::DataBuffer;
use node_engine::{CompiledGraph, SourceFramesMap, SourceTextsMap};
use node_frame_decoder::FrameParser;
use node_kind::NodeKind;
use node_gpu::{plan_unit, GpuSession, GpuUnitPlan};
use pipeline_bus::TopicKey;
use vofa_core::DataFrame;

use crate::eval_state::GraphEvalState;
use crate::graph_eval::{graph_triggered_by, EvalBreakdown, PutBack, SlotBufs, TakeGuard};

use super::plan::{
    build_plans, new_slot_bufs, ns_since, split_states, BucketPlan, PortAccum, ParallelPlans,
    WorkerState, EVAL_CHUNK,
};
use super::publish::{publish_batch_tail, publish_point, publish_port_batches};
use super::worker::{drain_worker, push_static_derived};

/// GPU 路径会话状态 — 版本化计划缓存 + 禁用闸 (挂于 `DataPlaneState`)
///
/// `session == None` 且 `version == graphs_version` 表示本版本已禁用
/// (无适配器 / 无资格单元 / 构建失败); 图重编译 (版本变化) 后重试一次。
pub struct GpuPathState {
    /// 已缓存计划/会话对应的 graphs_version
    version: u64,
    /// 就绪会话 (`None` = 本版本未启用)
    session: Option<GpuSession>,
    /// 每图 GPU 资格单元计划 (键 = 图 tab_id, 跨批迭代序稳定)
    plans: Vec<(Box<str>, Vec<Arc<GpuUnitPlan>>)>,
}

impl GpuPathState {
    /// 未尝试态 — `version` 取 u64::MAX 哨兵 (graphs_version 自 0 起, 首批必触发重建)
    #[must_use]
    pub const fn new() -> Self {
        Self {
            version: u64::MAX,
            session: None,
            plans: Vec::new(),
        }
    }
}

/// 单图 GPU 执行作业 — 上下行矩阵 + 读路径归属集
///
/// 每个触发图一个作业 (含无 GPU 资格单元的纯 CPU 图 — unit_ids 为空,
/// gpu_slots 为空集时 CpuOnly ≡ All, 保证纯 CPU 图照常评估)。
struct GpuJob {
    gi: usize,
    /// 会话键 = 图 tab_id
    id: String,
    /// GPU 承担的单元下标 (CPU 回退时这些单元也走 CPU)
    unit_ids: Vec<usize>,
    /// CPU 承担的单元下标 (gp.unit_ids − GPU 单元)
    cpu_unit_ids: Vec<usize>,
    /// 上传矩阵行序 = GPU 单元引用的 prelude 供给槽位 (升序)
    in_slots: Vec<u32>,
    /// GPU 输出槽位并集 (升序; 下载矩阵行序)
    out_slots: Vec<u32>,
    /// GPU 槽位集 (读路径 staging 归属判定)
    gpu_slots: BTreeSet<usize>,
    /// 上传矩阵 (slot-major: col × n)
    in_mat: Vec<f32>,
    /// 下载矩阵 (row × n)
    out_mat: Vec<f32>,
}

/// 读路径 staging 归属过滤
#[derive(Clone, Copy, PartialEq, Eq)]
enum StageMode {
    /// 全部 staging (CPU 回退模式)
    All,
    /// 仅 prelude/CPU 槽位
    CpuOnly,
    /// 仅 GPU 槽位
    GpuOnly,
}

/// 单图读路径 staging — 按 [`StageMode`] 过滤 (written 置位才入列)
#[allow(clippy::too_many_arguments)]
fn stage_graph_reads(
    plan: &BucketPlan,
    gi: usize,
    staged_derived: &mut Vec<(usize, f32)>,
    staged_spectra: &mut Vec<(u32, f32)>,
    ports: &mut [PortAccum],
    copy: &SlotBufs,
    frame_ts: u64,
    gpu_slots: &BTreeSet<usize>,
    mode: StageMode,
) {
    let want = |slot: usize| match mode {
        StageMode::All => true,
        StageMode::CpuOnly => !gpu_slots.contains(&slot),
        StageMode::GpuOnly => gpu_slots.contains(&slot),
    };
    let Some(gp) = plan.graphs.iter().find(|g| g.gi == gi) else {
        return;
    };
    for (slot, buf_idx) in &gp.derived {
        if want(*slot) && copy.1[*slot] {
            staged_derived.push((*buf_idx, copy.0[*slot]));
        }
    }
    for route in &gp.ports {
        let acc = &mut ports[*route];
        if want(acc.slot) && copy.1[acc.slot] {
            acc.timestamps.push(frame_ts);
            acc.values.push(f64::from(copy.0[acc.slot]));
        }
    }
    for &si in &gp.spectra {
        if let Some(slot) = plan.spectra[si].2 {
            if want(slot) && copy.1[slot] {
                staged_spectra.push((u32::try_from(si).unwrap_or(u32::MAX), copy.0[slot]));
            }
        }
    }
}

/// staging 断点 — GPU 批中失败时回滚本块已入列项 (改走 CPU 全量重评)
struct StageMark {
    derived: usize,
    spectra: usize,
    ports: Vec<(usize, usize)>,
}

fn stage_mark(ws: &WorkerState) -> StageMark {
    StageMark {
        derived: ws.staged_derived.len(),
        spectra: ws.staged_spectra.len(),
        ports: ws
            .ports
            .iter()
            .map(|p| (p.timestamps.len(), p.values.len()))
            .collect(),
    }
}

fn stage_rollback(ws: &mut WorkerState, mark: &StageMark) {
    ws.staged_derived.truncate(mark.derived);
    ws.staged_spectra.truncate(mark.spectra);
    for (p, (ts, vals)) in ws.ports.iter_mut().zip(&mark.ports) {
        p.timestamps.truncate(*ts);
        p.values.truncate(*vals);
    }
}

/// CPU 段 — 逐帧: prelude → CPU-only 单元 → CPU/全部读路径 staging;
/// GPU 供给槽位并行 gather 进上传矩阵
#[allow(clippy::too_many_arguments)]
fn run_cpu_section(
    plan: &BucketPlan,
    ws: &mut WorkerState,
    graph_list: &[&CompiledGraph],
    jobs: &mut [GpuJob],
    trigger_src_idx: &[Option<usize>],
    sf: &SourceFramesMap,
    texts: &SourceTextsMap,
    inputs: &HashMap<String, f32>,
    customs: &HashMap<String, HashMap<String, f32>>,
    decoders: &HashMap<String, FrameParser>,
    frames: &[DataFrame],
    chunk: (usize, usize),
    skip_gpu_units: bool,
) {
    let n = chunk.1 - chunk.0;
    for (fi, frame_i) in (chunk.0..chunk.1).enumerate() {
        let frame = &frames[frame_i];
        for job in jobs.iter_mut() {
            let g = graph_list[job.gi];
            let compiled = g.compiled();
            let copy = ws.copies.get_mut(&job.gi).expect("槽位副本已预分配");
            let resolved = compiled
                .resolve_frames(sf, trigger_src_idx[job.gi].map(|idx| (idx, frame)));
            let units = compiled.units();
            compiled.run_unit_frame(
                &units[0],
                resolved.as_slice(),
                texts,
                inputs,
                customs,
                &mut ws.filters,
                decoders,
                &mut ws.iffts,
                &mut ws.triggers,
                &mut copy.0,
                &mut copy.1,
                &mut copy.2,
                &mut copy.3,
            );
            // GPU 供给槽位 gather (prelude 写本副本之后; 缺失槽位 = 清零值 0.0,
            // 与 CPU Math 臂读取未写槽位语义一致)
            for (col, slot) in job.in_slots.iter().enumerate() {
                job.in_mat[col * n + fi] = copy.0[*slot as usize];
            }
            for &u in &job.cpu_unit_ids {
                compiled.run_unit_frame(
                    &units[u],
                    resolved.as_slice(),
                    texts,
                    inputs,
                    customs,
                    &mut ws.filters,
                    decoders,
                    &mut ws.iffts,
                    &mut ws.triggers,
                    &mut copy.0,
                    &mut copy.1,
                    &mut copy.2,
                    &mut copy.3,
                );
            }
            let mode = if skip_gpu_units {
                StageMode::All
            } else {
                StageMode::CpuOnly
            };
            stage_graph_reads(
                plan,
                job.gi,
                &mut ws.staged_derived,
                &mut ws.staged_spectra,
                &mut ws.ports,
                copy,
                frame.timestamp,
                &job.gpu_slots,
                mode,
            );
        }
    }
}

/// GPU 段 — 逐图 enqueue + 单次 submit + 阻塞回读 + GPU 槽位逐帧 staging
fn run_gpu_section(
    session: &mut GpuSession,
    plan: &BucketPlan,
    jobs: &mut [GpuJob],
    ws: &mut WorkerState,
    frames: &[DataFrame],
    chunk: (usize, usize),
) -> Result<(), gpu_core::GpuError> {
    let n = chunk.1 - chunk.0;
    for job in jobs.iter_mut() {
        if job.unit_ids.is_empty() {
            continue;
        }
        let frames_u32 = u32::try_from(n).unwrap_or(u32::MAX);
        let in_len = job.in_slots.len() * n;
        session.enqueue(&job.id, frames_u32, &job.in_mat[..in_len])?;
    }
    session.finish_chunk()?;
    for job in jobs.iter_mut() {
        if job.unit_ids.is_empty() {
            continue;
        }
        job.out_mat.clear();
        job.out_mat.resize(job.out_slots.len() * n, 0.0);
        session.read_out(&job.id, &mut job.out_mat);
        for (fi, frame_i) in (chunk.0..chunk.1).enumerate() {
            let frame = &frames[frame_i];
            let copy = ws.copies.get_mut(&job.gi).expect("槽位副本已预分配");
            for (row, slot) in job.out_slots.iter().enumerate() {
                copy.0[*slot as usize] = job.out_mat[row * n + fi];
                copy.1[*slot as usize] = true;
            }
            stage_graph_reads(
                plan,
                job.gi,
                &mut ws.staged_derived,
                &mut ws.staged_spectra,
                &mut ws.ports,
                copy,
                frame.timestamp,
                &job.gpu_slots,
                StageMode::GpuOnly,
            );
        }
    }
    Ok(())
}

/// GPU 批评估入口 — 返回 false 表示未走 GPU (调用方回退 CPU 路径)
///
/// 锁序与并行路径一致; `gpu` 由调用方持锁 (源间互斥)。
pub fn process_source_batch_gpu(
    eval_state: &GraphEvalState,
    gpu: &mut GpuPathState,
    source_frames: &mut SourceFramesMap,
    source_id: &str,
    frames: &[DataFrame],
    buffer: &mut DataBuffer,
    breakdown: &mut EvalBreakdown,
) -> bool {
    if frames.is_empty() {
        return false;
    }

    // —— 锁序: 与并行路径一致
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

    // 触发图划分 (与并行路径一致; 静态图走既有批级路径)
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
    if graph_list.is_empty() {
        return false;
    }

    // —— 会话/计划缓存 (版本变化时重建; 失败禁用至下个版本)
    if gpu.version != graphs_version {
        gpu.version = graphs_version;
        gpu.session = None;
        gpu.plans = build_graph_plans(&graph_list);
        if !gpu.plans.is_empty() {
            gpu.session = build_session(&gpu.plans);
        }
    }
    if gpu.session.is_none() || gpu.plans.is_empty() {
        return false;
    }
    eprintln!("PROBE: GPU path ENGAGED");
    log::info!(
        "GPU 评估路径启用: {} 图 / {} 资格单元 (version {graphs_version})",
        gpu.plans.len(),
        gpu.plans.iter().map(|(_, p)| p.len()).sum::<usize>()
    );

    // —— 单桶计划 (全部读路径归桶 0) + 状态切分 + 静态图批级评估
    let mut derived_edges: Vec<Vec<(usize, usize)>> = vec![Vec::new(); graph_list.len()];
    for (gi, g) in graph_list.iter().enumerate() {
        for e in g.edges() {
            if let Some(slot) = g.compiled().slot_of(&e.source, &e.source_handle) {
                derived_edges[gi].push((slot, buffer.derived_index_of(&e.target, &e.source)));
            }
        }
    }
    let ParallelPlans {
        arcs: plan_arcs,
        unit_bucket,
    } = build_plans(&graph_list, eval_state, &mut derived_edges, 1);
    let lead_plan = Arc::clone(&plan_arcs[0]);
    let mut ws = split_states(
        &plan_arcs,
        &graph_list,
        &mut filters_all,
        &mut iffts_all,
        &mut triggers_all,
    )
    .into_iter()
    .next()
    .expect("单桶必有状态");

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

    let mut static_edges: Vec<Vec<(usize, usize)>> = vec![Vec::new(); static_list.len()];
    for (g, edges) in static_list.iter().zip(&mut static_edges) {
        for e in g.edges() {
            if let Some(slot) = g.compiled().slot_of(&e.source, &e.source_handle) {
                edges.push((slot, buffer.derived_index_of(&e.target, &e.source)));
            }
        }
    }

    // 每图 GPU 作业 — 覆盖全部触发图 (纯 CPU 图 unit_ids 为空, 照常评估);
    // 矩阵按最大块预分配 + 触发源下标
    let plans_of: HashMap<&str, &Vec<Arc<GpuUnitPlan>>> = gpu
        .plans
        .iter()
        .map(|(id, p)| (id.as_ref(), p))
        .collect();
    let mut jobs: Vec<GpuJob> = graph_list
        .iter()
        .enumerate()
        .map(|(gi, g)| {
            let empty = &Vec::new();
            let plans = plans_of.get(g.tab_id.as_str()).copied().unwrap_or(empty);
            let out_slots: Vec<u32> = plans
                .iter()
                .flat_map(|p| p.out_slots.iter().copied())
                .collect::<BTreeSet<u32>>()
                .into_iter()
                .collect();
            let in_slots: BTreeSet<u32> = plans
                .iter()
                .flat_map(|p| p.in_slots.iter().copied())
                .collect();
            let gpu_unit: BTreeSet<usize> = plans.iter().map(|p| p.unit_index).collect();
            let all_units: Vec<usize> = (1..g.compiled().units().len()).collect();
            let cpu_unit_ids: Vec<usize> = all_units
                .iter()
                .copied()
                .filter(|u| !gpu_unit.contains(u))
                .collect();
            let cap = EVAL_CHUNK;
            let in_slots_vec: Vec<u32> = in_slots.into_iter().collect();
            let in_cap = in_slots_vec.len();
            GpuJob {
                gi,
                id: g.tab_id.clone(),
                unit_ids: gpu_unit.into_iter().collect(),
                cpu_unit_ids,
                in_slots: in_slots_vec,
                gpu_slots: out_slots.iter().map(|s| *s as usize).collect(),
                out_slots,
                in_mat: vec![0.0; in_cap * cap],
                out_mat: Vec::new(),
            }
        })
        .collect();
    let trigger_src_idx: Vec<Option<usize>> = graph_list
        .iter()
        .map(|g| {
            g.compiled()
                .frame_sources()
                .iter()
                .position(|s| s == source_id)
        })
        .collect();

    // 块区间 + 节流发布 (与并行路径一致)
    let mut chunks: Vec<(usize, usize)> = Vec::new();
    let mut cursor = 0;
    while cursor < frames.len() {
        let end = (cursor + EVAL_CHUNK).min(frames.len());
        chunks.push((cursor, end));
        cursor = end;
    }
    let publish_interval = std::time::Duration::from_millis(8);
    let mut last_publish = std::time::Instant::now();
    let mut gpu_failed = false;

    // —— 分块推进 (GPU 批中失败 → 回滚本块 staging, 全 CPU 续跑)
    for &(cs, ce) in &chunks {
        let mark = stage_mark(&ws);
        run_cpu_section(
            &lead_plan,
            &mut ws,
            &graph_list,
            &mut jobs,
            &trigger_src_idx,
            sf_map.get(),
            texts.get(),
            &inputs_guard,
            &customs_guard,
            decoders.get(),
            frames,
            (cs, ce),
            gpu_failed,
        );

        if !gpu_failed {
            let gpu_result = gpu.session.as_mut().map_or_else(
                || Err(gpu_core::GpuError::Map("会话已禁用".into())),
                |session| {
                    run_gpu_section(session, &lead_plan, &mut jobs, &mut ws, frames, (cs, ce))
                },
            );
            if let Err(e) = gpu_result {
                log::warn!("GPU 批中失败, 本批剩余回退 CPU: {e}");
                gpu_failed = true;
                gpu.session = None;
                stage_rollback(&mut ws, &mark);
                run_cpu_section(
                    &lead_plan,
                    &mut ws,
                    &graph_list,
                    &mut jobs,
                    &trigger_src_idx,
                    sf_map.get(),
                    texts.get(),
                    &inputs_guard,
                    &customs_guard,
                    decoders.get(),
                    frames,
                    (cs, ce),
                    true,
                );
            }
        }

        // 帧块推入 + staging 回放 + 节流发布 (与并行路径同序)
        let t = std::time::Instant::now();
        for frame in &frames[cs..ce] {
            buffer.push_frame(frame);
        }
        breakdown.push_frame_ns += ns_since(t);
        let t = std::time::Instant::now();
        drain_worker(&lead_plan, &mut ws, buffer, &mut analyzers, breakdown);
        push_static_derived(&static_edges, &static_bufs, (cs, ce), buffer);
        breakdown.derived_ns += ns_since(t);
        if last_publish.elapsed() >= publish_interval {
            publish_point(
                eval_state,
                &graph_list,
                std::slice::from_ref(&ws),
                &unit_bucket,
                &static_list,
                &static_bufs,
            );
            last_publish = std::time::Instant::now();
        }
    }

    // —— 批尾 (与并行路径同语义)
    publish_batch_tail(
        eval_state,
        graphs_version,
        &graph_list,
        std::slice::from_ref(&ws),
        &unit_bucket,
        &static_list,
        &static_bufs,
    );

    drop(analyzers);
    drop(triggers);
    drop(iffts);

    // 触发源最新帧写回缓存 (latest-value 融合)
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

    // 静态图批尾单样本收集 (锁内) → 与动态批一起发布
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

    publish_port_batches(eval_state, &plan_arcs, std::slice::from_ref(&ws), static_publish);
    true
}

/// 资格分析 — 每图收集 GPU 可编译单元 (全部为 Math 且读槽 ⊆ prelude ∪ 单元内);
/// 键 = 图 tab_id (CompiledGraph 唯一标识, 与图表迭代序解耦)
fn build_graph_plans(graph_list: &[&CompiledGraph]) -> Vec<(Box<str>, Vec<Arc<GpuUnitPlan>>)> {
    let mut out: Vec<(Box<str>, Vec<Arc<GpuUnitPlan>>)> = Vec::new();
    for g in graph_list {
        let compiled = g.compiled();
        let mut plans: Vec<Arc<GpuUnitPlan>> = Vec::new();
        for (u, unit) in compiled.units().iter().enumerate().skip(1) {
            if let Some(p) = plan_unit(0, compiled.ops(), unit, u, compiled.slot_unit()) {
                plans.push(Arc::new(p));
            }
        }
        if !plans.is_empty() {
            out.push((g.tab_id.as_str().into(), plans));
        }
    }
    out
}

/// 会话构建 — 无适配器返回 None; wgpu 管线创建对非法 WGSL panic,
/// codegen 测试已覆盖结构合法, catch_unwind 兜底回退 CPU
fn build_session(plans: &[(Box<str>, Vec<Arc<GpuUnitPlan>>)]) -> Option<GpuSession> {
    let ctx = gpu_core::GpuContext::acquire()?;
    let keyed: Vec<(String, Vec<Arc<GpuUnitPlan>>)> = plans
        .iter()
        .map(|(id, p)| (id.to_string(), p.clone()))
        .collect();
    std::panic::catch_unwind(AssertUnwindSafe(|| GpuSession::build(ctx, &keyed))).ok()
}
