//! 桶计划 — 类型定义 + 单元→桶 LPT 装箱 + 读路径归桶 + 状态切分

use std::collections::HashMap;
use std::sync::Arc;

use data_bus::TopicKey;
use dsp_fft::IfftState;
use dsp_filter::DigitalFilter;
use engine::{CompiledGraph, SourceFramesMap, SourceTextsMap};
use eval::CompiledEval;
use frame_decoder::FrameParser;
use kind::NodeKind;
use rustc_hash::FxHashMap;
use trigger::TriggerState;

use crate::eval_state::GraphEvalState;
use crate::graph_eval::SlotBufs;
use crate::graph_eval_parallel::simd::GraphSimdPlan;

/// 分块大小 — 每块一次屏障对; 高帧率下 8ms 快照节流不受影响 (4096 帧 @700k
/// fps ≈ 5.8ms < 8ms 门限), 低帧率批次天然单块
pub(super) const EVAL_CHUNK: usize = 4096;

/// 共享只读批上下文 — worker 跨线程借用
pub(super) struct BatchCtx<'a> {
    pub(super) graph_list: &'a [&'a CompiledGraph],
    pub(super) frames: &'a [vofa_core::DataFrame],
    pub(super) sf_map: &'a SourceFramesMap,
    pub(super) texts: &'a SourceTextsMap,
    pub(super) inputs: &'a HashMap<String, f32>,
    pub(super) customs: &'a HashMap<String, HashMap<String, f32>>,
    pub(super) decoders: &'a HashMap<String, FrameParser>,
    /// 每图触发源在 frame_sources 中的下标 (None = 不引用该源, 不做帧覆盖)
    pub(super) trigger_src_idx: &'a [Option<usize>],
    /// 中途快照发布标志 (协调者置位, worker 块尾采样)
    pub(super) publish_due: &'a std::sync::atomic::AtomicBool,
}

/// 单图在桶内的执行计划 (批内只读)
pub(super) struct BucketGraphPlan {
    pub(super) gi: usize,
    /// 本桶承担的计算单元下标 (不含 prelude; prelude 由所有涉及本图的桶本地复跑)
    pub(super) unit_ids: Vec<usize>,
    /// 本桶承担的标量单元下标 ([`Self::unit_ids`] 去除 SIMD 资格单元)
    pub(super) unit_ids_scalar: Vec<usize>,
    /// (正本槽位, buffer 派生索引) — 正本属本桶 (标量单元写出)
    pub(super) derived: Vec<(usize, usize)>,
    /// 同 [`Self::derived`], 正本槽位归 SIMD 资格单元 (staging 延后到 scatter)
    pub(super) derived_simd: Vec<(usize, usize)>,
    /// 端口批路由下标 (桶级扁平表 [`BucketPlan::port_routes`]; 标量单元写出槽位)
    pub(super) ports: Vec<usize>,
    /// 同 [`Self::ports`], 槽位归 SIMD 资格单元
    pub(super) ports_simd: Vec<usize>,
    /// 本图频谱项在 [`BucketPlan::spectra`] 中的下标 (标量单元写出槽位)
    pub(super) spectra: Vec<usize>,
    /// 同 [`Self::spectra`], 槽位归 SIMD 资格单元
    pub(super) spectra_simd: Vec<usize>,
    /// SIMD 批量求值计划 (`eval_simd` 关闭或无资格单元时为空)
    pub(super) simd: GraphSimdPlan,
}

/// 桶执行计划 — 只读路由表 (Arc 共享给 worker)
pub(super) struct BucketPlan {
    pub(super) graphs: Vec<BucketGraphPlan>,
    /// (图下标, sink_id, 正本槽位) — 归属本桶的频谱项
    pub(super) spectra: Vec<(usize, String, Option<usize>)>,
    /// 端口批路由: (topic, 正本槽位)
    pub(super) port_routes: Vec<(TopicKey, usize)>,
}

impl BucketPlan {
    pub(super) fn graph_mut(&mut self, gi: usize) -> &mut BucketGraphPlan {
        self.graphs
            .iter_mut()
            .find(|g| g.gi == gi)
            .expect("桶图计划已登记")
    }

    pub(super) fn ensure_graph(&mut self, gi: usize) {
        if !self.graphs.iter().any(|g| g.gi == gi) {
            self.graphs.push(BucketGraphPlan {
                gi,
                unit_ids: Vec::new(),
                unit_ids_scalar: Vec::new(),
                derived: Vec::new(),
                derived_simd: Vec::new(),
                ports: Vec::new(),
                ports_simd: Vec::new(),
                spectra: Vec::new(),
                spectra_simd: Vec::new(),
                simd: GraphSimdPlan::default(),
            });
        }
    }
}

/// worker 私有可变状态 — 批尾经归还箱交回协调者
pub(super) struct WorkerState {
    pub(super) copies: FxHashMap<usize, SlotBufs>,
    /// SIMD 列工作区 (图下标 → 槽位列缓存; 跨块复用容量)
    pub(super) simd_cols: FxHashMap<usize, crate::graph_eval_parallel::simd::ops::SimdWorkspace>,
    pub(super) filters: HashMap<String, DigitalFilter>,
    pub(super) iffts: HashMap<String, IfftState>,
    pub(super) triggers: HashMap<String, TriggerState>,
    /// 与 [`BucketPlan::port_routes`] 按下标对齐
    pub(super) ports: Vec<PortAccum>,
    /// 派生 staging (buffer 派生索引, 值) — 每块与协调者交换
    pub(super) staged_derived: Vec<(usize, u64, f32)>,
    /// 频谱 staging (桶级 spectra 表下标, 值)
    pub(super) staged_spectra: Vec<(u32, f32)>,
    /// 快照物化增量 (发布标志置位时块尾物化, 随 staging 交换带回)
    pub(super) snapshot_delta: Option<(engine::ValuesMap, engine::StringValuesMap)>,
}

impl WorkerState {
    pub(super) fn new(plan: &BucketPlan) -> Self {
        Self {
            copies: FxHashMap::default(),
            simd_cols: FxHashMap::default(),
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

/// 端口批累积 — 每帧 written 置位才追加 (与串行版 port_batches 语义一致)
pub(super) struct PortAccum {
    pub(super) route: usize,
    pub(super) slot: usize,
    pub(super) timestamps: Vec<u64>,
    pub(super) values: Vec<f64>,
}

/// 桶计划集 — LPT 装箱 + 读路径归桶产物
pub(super) struct ParallelPlans {
    pub(super) arcs: Vec<Arc<BucketPlan>>,
    /// 每图单元 → 桶下标 (回放/发布按桶取副本)
    pub(super) unit_bucket: Vec<Vec<u32>>,
}

/// 单元→桶 LPT 装箱 + 读路径归桶 (端口批 / 派生边 / 频谱项)
///
/// 权重降序 → 最轻桶, 批次确定性; 正本槽位按 "槽位 → 所属单元 → 桶" 分派
/// 到恰好一个桶。
pub(super) fn build_plans(
    graph_list: &[&CompiledGraph],
    eval_state: &GraphEvalState,
    derived_edges: &mut [Vec<(usize, usize)>],
    eval_workers: usize,
    simd: bool,
) -> ParallelPlans {
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

    // 读路径归桶: 正本槽位 → 所属单元 → 桶 (端口批 / 派生边 / 频谱项)
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

    // SIMD 分区: 资格分析 + 标量/SIMD 单元与读路径切分 (simd 关闭时全标量)
    for plan in &mut plans {
        for gp in &mut plan.graphs {
            let compiled = graph_list[gp.gi].compiled();
            if simd {
                gp.simd = GraphSimdPlan::build(compiled);
            }
            let simd_out = &gp.simd.out_slots;
            let is_simd_slot = |slot: usize| simd_out.binary_search(&slot).is_ok();
            let simd_units = &gp.simd.units;
            gp.unit_ids_scalar = gp
                .unit_ids
                .iter()
                .copied()
                .filter(|u| !simd_units.iter().any(|p| p.unit_index == *u))
                .collect();
            let mut derived_simd = std::mem::take(&mut gp.derived_simd);
            let mut derived_scalar = Vec::with_capacity(gp.derived.len());
            for entry in std::mem::take(&mut gp.derived) {
                if is_simd_slot(entry.0) {
                    derived_simd.push(entry);
                } else {
                    derived_scalar.push(entry);
                }
            }
            gp.derived = derived_scalar;
            gp.derived_simd = derived_simd;
            let mut ports_simd = std::mem::take(&mut gp.ports_simd);
            let mut ports_scalar = Vec::with_capacity(gp.ports.len());
            for route in std::mem::take(&mut gp.ports) {
                let slot = plan.port_routes[route].1;
                if is_simd_slot(slot) {
                    ports_simd.push(route);
                } else {
                    ports_scalar.push(route);
                }
            }
            gp.ports = ports_scalar;
            gp.ports_simd = ports_simd;
            let mut spectra_simd = std::mem::take(&mut gp.spectra_simd);
            let mut spectra_scalar = Vec::with_capacity(gp.spectra.len());
            for si in std::mem::take(&mut gp.spectra) {
                if plan.spectra[si].2.is_some_and(&is_simd_slot) {
                    spectra_simd.push(si);
                } else {
                    spectra_scalar.push(si);
                }
            }
            gp.spectra = spectra_scalar;
            gp.spectra_simd = spectra_simd;
        }
    }

    ParallelPlans {
        arcs: plans.into_iter().map(Arc::new).collect(),
        unit_bucket,
    }
}

/// 状态切分 — 按单元 id 表 drain 出每桶子 map (未认领条目留原表, 批尾合并写回)
pub(super) fn split_states(
    plans: &[Arc<BucketPlan>],
    graph_list: &[&CompiledGraph],
    filters_all: &mut HashMap<String, DigitalFilter>,
    iffts_all: &mut HashMap<String, IfftState>,
    triggers_all: &mut HashMap<String, TriggerState>,
) -> Vec<WorkerState> {
    let mut worker_states: Vec<WorkerState> = plans
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
    for (b, plan) in plans.iter().enumerate() {
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
    worker_states
}

/// 正本槽位 → 桶 (槽位 → 单元 → 桶)
pub(super) fn bucket_of(unit_bucket: &[u32], slot_unit: &[u32], slot: usize) -> usize {
    unit_bucket[slot_unit[slot] as usize] as usize
}

pub(super) fn new_slot_bufs(compiled: &CompiledEval) -> SlotBufs {
    (
        vec![0.0; compiled.slot_count()],
        vec![false; compiled.slot_count()],
        vec![String::new(); compiled.str_slot_count()],
        vec![false; compiled.str_slot_count()],
    )
}

pub(super) fn ns_since(t: std::time::Instant) -> u64 {
    u64::try_from(t.elapsed().as_nanos()).unwrap_or(u64::MAX)
}
