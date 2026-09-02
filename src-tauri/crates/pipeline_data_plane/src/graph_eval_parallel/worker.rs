//! worker 执行体 — 单桶单块评估 + staging 回放 + 快照物化 + 静态图派生

use std::collections::HashMap;

use buffer_databuffer::DataBuffer;
use dsp_fft::SpectrumAnalyzer;

use crate::graph_eval::{EvalBreakdown, SlotBufs};
use crate::graph_eval_parallel::simd;

use super::plan::{ns_since, BatchCtx, BucketPlan, WorkerState};

/// 单桶单块执行 — 图优先两相推进:
///
/// 相 1 (逐帧): prelude → 标量单元 → gather (SIMD 输入槽位) → 标量 staging
/// 相 2 (块内批量): SIMD 资格单元 SoA 求值 → scatter 逐帧回写 + 延后 staging
///
/// 单元写集互斥 (编译期切分不变量; SIMD 单元读仅 prelude/本单元), 两相与
/// 旧逐帧单相拓扑可观测输出一致: 派生环/端口批按槽位归属分区后各自帧序
/// 不变, 快照物化读块尾副本状态不变。
pub(super) fn run_bucket_chunk(
    plan: &BucketPlan,
    ws: &mut WorkerState,
    ctx: &BatchCtx<'_>,
    chunk: (usize, usize),
) {
    for gp in &plan.graphs {
        let g = ctx.graph_list[gp.gi];
        let compiled = g.compiled();
        let copy = ws.copies.get_mut(&gp.gi).expect("槽位副本已在批首预分配");
        let units = compiled.units();
        let n = chunk.1 - chunk.0;
        let simd_plan = &gp.simd;
        let has_simd = !simd_plan.is_empty();
        if has_simd {
            let col_ws = ws.simd_cols.entry(gp.gi).or_default();
            for &slot in simd_plan.in_slots.iter().chain(simd_plan.out_slots.iter()) {
                col_ws.ensure_col(slot, n);
            }
        }

        // —— 相 1: 逐帧 prelude + 标量单元 + gather + 标量 staging
        for (f, frame_i) in (chunk.0..chunk.1).enumerate() {
            let frame = &ctx.frames[frame_i];
            let resolved = compiled.resolve_frames(
                ctx.sf_map,
                ctx.trigger_src_idx[gp.gi].map(|idx| (idx, frame)),
            );
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
            for &u in &gp.unit_ids_scalar {
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
            if has_simd {
                // gather: SIMD 输入槽位 (prelude 供给) 逐帧收进 SoA 列
                let col_ws = ws.simd_cols.get_mut(&gp.gi).expect("列工作区已预建");
                for &slot in &simd_plan.in_slots {
                    col_ws.gather(slot, f, copy.0[slot]);
                }
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

        // —— 相 2: SIMD 单元批量求值 + scatter 逐帧回写 + 延后 staging
        if has_simd {
            let col_ws = ws.simd_cols.get_mut(&gp.gi).expect("列工作区已预建");
            for unit in &simd_plan.units {
                simd::ops::apply_unit(unit, col_ws, n);
            }
            let col_ws = ws.simd_cols.get(&gp.gi).expect("列工作区已预建");
            for (f, frame_i) in (chunk.0..chunk.1).enumerate() {
                let frame = &ctx.frames[frame_i];
                for &slot in &simd_plan.out_slots {
                    copy.0[slot] = col_ws.col(slot)[f];
                    copy.1[slot] = true;
                }
                for (slot, buf_idx) in &gp.derived_simd {
                    if copy.1[*slot] {
                        ws.staged_derived.push((*buf_idx, copy.0[*slot]));
                    }
                }
                for route in &gp.ports_simd {
                    let acc = &mut ws.ports[*route];
                    let slot = acc.slot;
                    if copy.1[slot] {
                        acc.timestamps.push(frame.timestamp);
                        acc.values.push(f64::from(copy.0[slot]));
                    }
                }
                for &si in &gp.spectra_simd {
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
}

/// 回放单桶 staging — 派生 → buffer; 频谱 → analyzer (块间由屏障定序)
pub(super) fn drain_worker(
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
pub(super) fn materialize_bucket(
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
pub(super) fn push_static_derived(
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
