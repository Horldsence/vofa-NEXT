//! 快照物化 — 槽位 → ValuesMap (仅快照发布点调用, 非逐帧)

use super::CompiledEval;
use crate::eval_ports::{node_out_entry, set_port};
use crate::eval_str::{node_out_str_entry, set_str_port};
use crate::{StringValuesMap, ValuesMap};

impl CompiledEval {
    /// 快照物化: slots + written → ValuesMap (仅快照发布点调用, 非逐帧)
    ///
    /// 只覆盖写本帧已产出的端口, 不清理过期键 (与 evaluate_into 语义一致)
    pub fn materialize(&self, slots: &[f32], written: &[bool], out: &mut ValuesMap) {
        for (i, (node_id, port)) in self.plan.slot_names.iter().enumerate() {
            if written[i] {
                let m = node_out_entry(out, node_id);
                set_port(m, port, slots[i]);
            }
        }
    }

    /// 数值槽位与 `(node_id, port)` 的稳定对应表。
    pub fn slot_names(&self) -> &[(String, String)] {
        &self.plan.slot_names
    }

    /// 字符串快照物化: str_slots + str_written → StringValuesMap (仅快照发布点调用)
    ///
    /// 只物化 written 置位的槽位, 不清理过期键 (与 materialize / evaluate_into 语义一致)
    pub fn materialize_str(
        &self,
        str_slots: &[String],
        str_written: &[bool],
        out_str: &mut StringValuesMap,
    ) {
        for (i, (node_id, port)) in self.plan.str_slot_names.iter().enumerate() {
            if str_written[i] {
                let m = node_out_str_entry(out_str, node_id);
                set_str_port(m, port, &str_slots[i]);
            }
        }
    }

    /// Fft 输入: (sink_id, value) 迭代, 仅 written 槽位
    pub fn spectrum_values<'a>(
        &'a self,
        slots: &'a [f32],
        written: &'a [bool],
    ) -> impl Iterator<Item = (&'a str, f32)> + 'a {
        self.plan
            .spectrum_slots
            .iter()
            .filter_map(move |(sink, slot)| match slot {
                Some(s) if written[*s] => Some((sink.as_str(), slots[*s])),
                _ => None,
            })
    }

    /// 单元快照物化 — 仅物化本单元写槽位中 written 置位者 (并发路径按
    /// 单元→桶副本读取; 与全表 [`Self::materialize`] 产出相同的键值集)
    pub fn materialize_unit(
        &self,
        unit: &lower::EvalUnit,
        slots: &[f32],
        written: &[bool],
        out: &mut ValuesMap,
    ) {
        for &s in &unit.clear_slots {
            let i = s as usize;
            if written[i] {
                let (node_id, port) = &self.plan.slot_names[i];
                let m = node_out_entry(out, node_id);
                set_port(m, port, slots[i]);
            }
        }
    }

    /// 单元字符串快照物化 (语义同 [`Self::materialize_str`] 的单元化版本)
    pub fn materialize_str_unit(
        &self,
        unit: &lower::EvalUnit,
        str_slots: &[String],
        str_written: &[bool],
        out_str: &mut StringValuesMap,
    ) {
        for &s in &unit.clear_str_slots {
            let i = s as usize;
            if str_written[i] {
                let (node_id, port) = &self.plan.str_slot_names[i];
                let m = node_out_str_entry(out_str, node_id);
                set_str_port(m, port, &str_slots[i]);
            }
        }
    }
}
