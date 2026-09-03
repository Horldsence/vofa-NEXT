//! 快照发布 — 节流点 / 批尾发布 + DataBus 端口批发布

use std::collections::HashMap;
use std::sync::Arc;

use data_bus::TopicKey;
use engine::CompiledGraph;

use crate::eval_state::GraphEvalState;
use crate::graph_eval::{merge_str_map, SlotBufs};

use super::plan::{BucketPlan, WorkerState};

/// 快照发布点 — 物化全部图 + 字符串输出 (节流由调用方判定)
pub(super) fn publish_point(
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
pub(super) fn publish_snapshot_values(
    graph_list: &[&CompiledGraph],
    worker_states: &[WorkerState],
    unit_bucket: &[Vec<u32>],
    static_list: &[&CompiledGraph],
    static_bufs: &[SlotBufs],
    out: &mut engine::ValuesMap,
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
pub(super) fn publish_str(
    graph_list: &[&CompiledGraph],
    worker_states: &[WorkerState],
    unit_bucket: &[Vec<u32>],
    static_list: &[&CompiledGraph],
    static_bufs: &[SlotBufs],
    out: &mut HashMap<String, HashMap<String, String>>,
) {
    let mut buf = engine::StringValuesMap::default();
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

/// 批尾最终发布 — 版本变化先清空 (过期键不回流), 返回版本变化标志
pub(super) fn publish_batch_tail(
    eval_state: &GraphEvalState,
    graphs_version: u64,
    graph_list: &[&CompiledGraph],
    worker_states: &[WorkerState],
    unit_bucket: &[Vec<u32>],
    static_list: &[&CompiledGraph],
    static_bufs: &[SlotBufs],
) -> bool {
    let version_changed = {
        let mut snap = eval_state.output_snapshot.lock();
        let changed = snap.graphs_version != graphs_version;
        if changed {
            snap.values.clear();
            snap.graphs_version = graphs_version;
        }
        publish_snapshot_values(
            graph_list,
            worker_states,
            unit_bucket,
            static_list,
            static_bufs,
            &mut snap.values,
        );
        snap.tick = snap.tick.wrapping_add(1);
        changed
    };
    if version_changed {
        eval_state.graph_string_outputs.lock().clear();
    }
    publish_str(
        graph_list,
        worker_states,
        unit_bucket,
        static_list,
        static_bufs,
        &mut eval_state.graph_string_outputs.lock(),
    );
    version_changed
}

/// DataBus 端口批发布 (桶序 → 路由序; 静态图批尾单样本)
pub(super) fn publish_port_batches(
    eval_state: &GraphEvalState,
    plan_arcs: &[Arc<BucketPlan>],
    worker_states: &[WorkerState],
    static_publish: Vec<(TopicKey, u64, f64)>,
) {
    for (b, ws) in worker_states.iter().enumerate() {
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
