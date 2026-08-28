//! Str arm — input_ports 拆分 str/num → op.evaluate → StrResult 分派
//! str ≤ 2 / num ≤ 2 走栈数组,超出走堆兜底;StrOp::evaluate 的 str_inputs 紧凑对齐
//! 未连接字符串端口的内联回退: 仅 FORMAT 的 "fmt" 端口取 `tmpl` 参数 (对称于快路径 str_defaults)

use node_kind::{str_num_default, uses_str_inline_text, NodeKind, PortDomain, StrResult};

use crate::compile::CompiledGraph;
use node_eval::{node_out_entry, node_out_str_entry, set_port, set_str_port};

use super::{EvalCtx, NodeArm};

pub struct StrArm;

impl NodeArm for StrArm {
    fn run(&self, graph: &CompiledGraph, node_id: &str, ctx: &mut EvalCtx<'_>) {
        let Some(node) = graph.value_def(node_id) else {
            return;
        };
        let NodeKind::Str { op, num, tmpl } = &node.kind else {
            return;
        };
        let ports = op.input_ports();
        let n_str = ports.iter().filter(|p| p.1 == PortDomain::String).count();
        let n_num = ports.len() - n_str;
        let mut stack_str: [&str; 2] = ["", ""];
        let mut heap_str;
        let str_inputs: &mut [&str] = if n_str <= 2 {
            &mut stack_str[..n_str]
        } else {
            heap_str = vec![""; n_str];
            &mut heap_str
        };
        let mut stack_num = [0.0f32; 2];
        let mut heap_num;
        let num_inputs: &mut [f32] = if n_num <= 2 {
            &mut stack_num[..n_num]
        } else {
            heap_num = vec![0.0; n_num];
            &mut heap_num
        };
        let (mut si, mut ni) = (0, 0);
        for (name, domain) in ports {
            match domain {
                PortDomain::String => {
                    // 已连接走上游快照; 未连接走内联回退文本 (仅 fmt ← tmpl)
                    let linked = graph
                        .string_input_index
                        .get(node_id)
                        .is_some_and(|p| p.contains_key(*name));
                    str_inputs[si] = if linked {
                        graph.resolve_str_input(node_id, name, ctx.out_str)
                    } else if uses_str_inline_text(*op, name) {
                        tmpl.as_str()
                    } else {
                        ""
                    };
                    si += 1;
                }
                PortDomain::F32 => {
                    num_inputs[ni] = if graph
                        .input_index
                        .get(node_id)
                        .is_some_and(|p| p.contains_key(*name))
                    {
                        graph.resolve_input(node_id, name, ctx.out)
                    } else {
                        str_num_default(num, name)
                    };
                    ni += 1;
                }
                PortDomain::Bytes => {}
            }
        }
        match op.evaluate(str_inputs, num_inputs) {
            StrResult::Text(t) => {
                set_str_port(node_out_str_entry(ctx.out_str, node_id), "result", &t);
            }
            StrResult::Num(v) => {
                set_port(node_out_entry(ctx.out, node_id), "result", v);
            }
        }
    }
}
