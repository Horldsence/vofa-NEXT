//! 慢路径图求值 — CompiledGraph::evaluate / evaluate_into + NodeArm 分发表
//!
//! 主循环走 [`arm_for`] 按 [`kind::NodeKind`] variant 分派到 [`arms`] 中对应的
//! NodeArm impl;arm 函数签名 `run(graph, node_id, ctx)` — graph 与 node_id 独立于 ctx,
//! 避免同一表达式内 mut borrow ctx.out + immut borrow ctx.node.id 的借用冲突。
//! 所有可变运行期状态 (out / out_str / filter_states / ifft_states / trigger_states) 集中
//! 在 [`EvalCtx`],arm 通过 [`crate::eval`] 中已有 helper (node_out_entry / set_port /
//! set_str_port / str_num_default) 与 [`CompiledGraph`] 上游反查访问器
//! (resolve_input / resolve_str_input) 协作,无新数据、无新枚举。

use std::collections::HashMap;

use dsp_fft::IfftState;
use dsp_filter::DigitalFilter;
use frame_decoder::FrameParser;
use kind::NodeKind;
use trigger::TriggerState;

use eval::{SourceFramesMap, SourceTextsMap, StringValuesMap, ValuesMap};

use crate::compile::CompiledGraph;

pub mod arms;

pub use arms::arm_for;

/// 求值上下文 — 持有 evaluate_into 的全部输入与可变输出借用,不含 graph 与 node_id
/// (二者作为 arm.run 的独立参数传入,避免与 ctx 的可变字段借用冲突)
pub struct EvalCtx<'a> {
    pub source_frames: &'a SourceFramesMap,
    pub source_texts: &'a SourceTextsMap,
    pub input_values: &'a HashMap<String, f32>,
    pub custom_outputs: &'a HashMap<String, HashMap<String, f32>>,
    pub decoder_states: &'a HashMap<String, FrameParser>,
    pub filter_states: &'a mut HashMap<String, DigitalFilter>,
    pub ifft_states: &'a mut HashMap<String, IfftState>,
    pub trigger_states: &'a mut HashMap<String, TriggerState>,
    pub out: &'a mut ValuesMap,
    pub out_str: &'a mut StringValuesMap,
}

/// 求值 arm 抽象 — 每个 NodeKind variant 一个 unit struct impl,按 [`arm_for`] 分派
pub trait NodeArm: Send + Sync {
    fn run(&self, graph: &CompiledGraph, node_id: &str, ctx: &mut EvalCtx<'_>);
}

impl CompiledGraph {
    pub fn evaluate(
        &self,
        source_frames: &SourceFramesMap,
        source_texts: &SourceTextsMap,
        input_values: &HashMap<String, f32>,
        custom_outputs: &HashMap<String, HashMap<String, f32>>,
        filter_states: &mut HashMap<String, DigitalFilter>,
        decoder_states: &HashMap<String, FrameParser>,
        ifft_states: &mut HashMap<String, IfftState>,
        trigger_states: &mut HashMap<String, TriggerState>,
        out_str: &mut StringValuesMap,
    ) -> ValuesMap {
        let mut out = ValuesMap::default();
        self.evaluate_into(
            source_frames,
            source_texts,
            input_values,
            custom_outputs,
            filter_states,
            decoder_states,
            ifft_states,
            trigger_states,
            &mut out,
            out_str,
        );
        out
    }

    #[allow(clippy::too_many_arguments, clippy::cast_precision_loss)]
    pub fn evaluate_into(
        &self,
        source_frames: &SourceFramesMap,
        source_texts: &SourceTextsMap,
        input_values: &HashMap<String, f32>,
        custom_outputs: &HashMap<String, HashMap<String, f32>>,
        filter_states: &mut HashMap<String, DigitalFilter>,
        decoder_states: &HashMap<String, FrameParser>,
        ifft_states: &mut HashMap<String, IfftState>,
        trigger_states: &mut HashMap<String, TriggerState>,
        out: &mut ValuesMap,
        out_str: &mut StringValuesMap,
    ) {
        for node_id in &self.eval_order {
            let Some(node) = self.value_def(node_id) else {
                continue;
            };
            let Some(arm) = arm_for(&node.kind) else {
                continue;
            };
            let mut ctx = EvalCtx {
                source_frames,
                source_texts,
                input_values,
                custom_outputs,
                decoder_states,
                filter_states,
                ifft_states,
                trigger_states,
                out,
                out_str,
            };
            arm.run(self, node_id, &mut ctx);
        }
    }

    pub fn collect_custom_inputs(
        &self,
        computed: &ValuesMap,
    ) -> HashMap<String, HashMap<String, f32>> {
        let mut result = HashMap::new();
        for node in self.value_nodes() {
            if let NodeKind::Custom { inputs, .. } = &node.kind {
                let m = inputs
                    .iter()
                    .map(|port| (port.clone(), self.resolve_input(&node.id, port, computed)))
                    .collect();
                result.insert(node.id.clone(), m);
            }
        }
        result
    }

    pub fn collect_spectrum_inputs(&self, computed: &ValuesMap) -> HashMap<String, f32> {
        let mut result = HashMap::new();
        for node in self.value_nodes() {
            if matches!(node.kind, NodeKind::SpectrumSink { .. }) {
                result.insert(
                    node.id.clone(),
                    self.resolve_input(&node.id, "in0", computed),
                );
            }
        }
        result
    }

    pub fn resolve_input(&self, node_id: &str, port_id: &str, computed: &ValuesMap) -> f32 {
        if let Some((src_node, src_port)) = self
            .input_index
            .get(node_id)
            .and_then(|ports| ports.get(port_id))
        {
            computed
                .get(src_node)
                .and_then(|m| m.get(src_port))
                .copied()
                .unwrap_or(0.0)
        } else {
            0.0
        }
    }

    pub fn resolve_str_input<'a>(
        &self,
        node_id: &str,
        port_id: &str,
        computed_str: &'a StringValuesMap,
    ) -> &'a str {
        if let Some((src_node, src_port)) = self
            .string_input_index
            .get(node_id)
            .and_then(|ports| ports.get(port_id))
        {
            computed_str
                .get(src_node)
                .and_then(|m| m.get(src_port))
                .map_or("", String::as_str)
        } else {
            ""
        }
    }
}
