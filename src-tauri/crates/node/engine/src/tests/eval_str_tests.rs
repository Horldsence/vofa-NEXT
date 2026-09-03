//! Str 节点求值测试 (含 TextInput)
#![allow(unused_imports, dead_code)]

use dsp_filter::{DigitalFilter, FilterConfig};
use kind::{MathOp, StrNumParams, StrOp};
use trigger::TriggerMatchType;

use super::*;
use crate::compile::CompiledGraph;
use testkit::*;
#[test]
fn test_str_len_outputs_f32_to_values_map() {
    // Len 输出域为 F32: 写入 ValuesMap, 不写 StringValuesMap;
    // 未连接字符串输入按 "" → 长度 0
    let nodes = vec![make_str("len1", "t1", StrOp::Len)];
    let g = CompiledGraph::compile("t1".into(), nodes, vec![]).unwrap();
    let mut out_str = StringValuesMap::default();
    let out = g.evaluate(
        &empty_frames(),
        &empty_texts(),
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut out_str,
    );
    assert_eq!(out.get("len1").and_then(|m| m.get("result")), Some(&0.0));
    assert!(!out_str.contains_key("len1"));
}

#[test]
fn test_str_find_contains_on_empty_defaults() {
    // Find/Contains 输出 F32; 未连接输入按 "": "".find("") 命中位置 1, "".contains("") 为真
    let nodes = vec![
        make_str("find1", "t1", StrOp::Find),
        make_str("contains1", "t1", StrOp::Contains),
    ];
    let g = CompiledGraph::compile("t1".into(), nodes, vec![]).unwrap();
    let out = g.evaluate(
        &empty_frames(),
        &empty_texts(),
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut StringValuesMap::default(),
    );
    assert_eq!(out.get("find1").and_then(|m| m.get("result")), Some(&1.0));
    assert_eq!(
        out.get("contains1").and_then(|m| m.get("result")),
        Some(&1.0)
    );
}

#[test]
fn test_str_text_output_written_to_str_map() {
    // Mid/Replace 输出 String: 写入 out_str[node]["result"], 不写 ValuesMap;
    // 未连接字符串输入按 "" → 输出 ""
    let nodes = vec![
        make_str_num(
            "mid1",
            "t1",
            StrOp::Mid,
            StrNumParams {
                pos: 2.0,
                len: 1.0,
                size: 0.0,
            },
        ),
        make_str("rep1", "t1", StrOp::Replace),
    ];
    let g = CompiledGraph::compile("t1".into(), nodes, vec![]).unwrap();
    let mut out_str = StringValuesMap::default();
    let out = g.evaluate(
        &empty_frames(),
        &empty_texts(),
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut out_str,
    );
    assert_eq!(
        out_str.get("mid1").and_then(|m| m.get("result")),
        Some(&String::new())
    );
    assert_eq!(
        out_str.get("rep1").and_then(|m| m.get("result")),
        Some(&String::new())
    );
    assert!(!out.contains_key("mid1"));
    assert!(!out.contains_key("rep1"));
}

#[test]
fn test_str_num_port_fallback_vs_connected() {
    // Mid 的 pos/len 端口:
    // - 未连接 (len) → 编译期捕获 num 内联回退值, num_inputs 为 None
    // - 已连接 (pos ← Input.value) → num_inputs 为 Some (走上游值)
    let nodes = vec![
        make_input("knob1", "t1"),
        make_str_num(
            "mid1",
            "t1",
            StrOp::Mid,
            StrNumParams {
                pos: 9.0,
                len: 3.0,
                size: 0.0,
            },
        ),
    ];
    let edges = vec![edge("e1", "knob1", "value", "mid1", "pos")];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();

    // 编译期结构断言: num_inputs/num_defaults 与端口表 F32 端口 (pos, len) 紧凑对齐
    let str_op = g
        .compiled()
        .ops()
        .iter()
        .find_map(|op| match op {
            CompiledOp::Str {
                num_inputs,
                num_defaults,
                ..
            } => Some((num_inputs, num_defaults)),
            _ => None,
        })
        .expect("应有 Str op");
    assert_eq!(str_op.0.len(), 2);
    assert!(str_op.0[0].is_some(), "pos 已连接应解析到上游槽位");
    assert!(str_op.0[1].is_none(), "len 未连接应为 None");
    assert_eq!(
        str_op.1,
        &[9.0, 3.0],
        "回退值应按端口名映射 num.pos/num.len"
    );

    // 行为: 求值不崩溃, 输出写入 out_str (输入为 "" 故结果 "")
    let mut input_values = HashMap::new();
    input_values.insert("knob1".to_string(), 2.0_f32);
    let mut out_str = StringValuesMap::default();
    g.evaluate(
        &empty_frames(),
        &empty_texts(),
        &input_values,
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut out_str,
    );
    assert!(out_str.contains_key("mid1"));
}

#[test]
fn test_str_chain_two_nodes() {
    // 两个 Str 串联: Concat(str1,str2 未连接 → "") → Upper → 字符串值沿边路由
    // 再经 Len (String→F32) 验证字符串平面结果可被数值平面消费
    let nodes = vec![
        make_str("concat1", "t1", StrOp::Concat),
        make_str("up1", "t1", StrOp::Upper),
        make_str("len1", "t1", StrOp::Len),
    ];
    let edges = vec![
        edge("e1", "concat1", "result", "up1", "str"),
        edge("e2", "up1", "result", "len1", "str"),
    ];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let mut out_str = StringValuesMap::default();
    let out = g.evaluate(
        &empty_frames(),
        &empty_texts(),
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut out_str,
    );
    assert_eq!(
        out_str.get("concat1").and_then(|m| m.get("result")),
        Some(&String::new())
    );
    assert_eq!(
        out_str.get("up1").and_then(|m| m.get("result")),
        Some(&String::new())
    );
    // Len("".to_uppercase()) = 0 — 证明字符串沿 string_edges 路由且拓扑序正确
    // (若顺序错误, Len 读到上游未求值的缺省 "" 也是 0, 故另断言 eval_order)
    assert_eq!(out.get("len1").and_then(|m| m.get("result")), Some(&0.0));
    let pos = |id: &str| g.eval_order.iter().position(|n| n == id).unwrap();
    assert!(pos("concat1") < pos("up1"));
    assert!(pos("up1") < pos("len1"));
}

#[test]
fn test_text_input_writes_str_port_slow_path() {
    // 慢路径: 参数 text 原样写入 out_str[node_id]["str"];
    // TextInput → Str(Upper) 验证字符串经 string_edges 流向下游 (拓扑序正确)
    let nodes = vec![
        make_text_input("ti1", "t1", "hello"),
        make_str("up1", "t1", StrOp::Upper),
    ];
    let edges = vec![edge("e1", "ti1", "str", "up1", "str")];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    // 编译期槽位: "str" 为字符串槽位, 不占数值槽位
    assert!(g.compiled().str_slot_of("ti1", "str").is_some());
    assert!(g.compiled().slot_of("ti1", "str").is_none());

    let mut out_str = StringValuesMap::default();
    let out = g.evaluate(
        &empty_frames(),
        &empty_texts(),
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut out_str,
    );
    assert_eq!(
        out_str.get("ti1").and_then(|m| m.get("str")),
        Some(&"hello".to_string())
    );
    // 下游 Upper 读到非空文本 → "HELLO" (否则读到缺省 "" 输出 "")
    assert_eq!(
        out_str.get("up1").and_then(|m| m.get("result")),
        Some(&"HELLO".to_string())
    );
    // TextInput 无数值平面输出
    assert!(!out.contains_key("ti1"));
}

#[test]
fn test_text_input_slot_run_matches_slow_path() {
    // 快路径 (compiled.run + materialize_str) 与慢路径同语义:
    // TextInput("hello") → Str(Len) → 数值平面 5
    let nodes = vec![
        make_text_input("ti1", "t1", "hello"),
        make_str("len1", "t1", StrOp::Len),
    ];
    let edges = vec![edge("e1", "ti1", "str", "len1", "str")];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();

    // 慢路径
    let mut out_str_a = StringValuesMap::default();
    let out_a = g.evaluate(
        &empty_frames(),
        &empty_texts(),
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut out_str_a,
    );

    // 快路径
    let compiled = g.compiled();
    let mut slots = vec![0.0f32; compiled.slot_count()];
    let mut written = vec![false; compiled.slot_count()];
    let mut str_slots = vec![String::new(); compiled.str_slot_count()];
    let mut str_written = vec![false; compiled.str_slot_count()];
    compiled.run(
        &empty_frames(),
        &empty_texts(),
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut slots,
        &mut written,
        &mut str_slots,
        &mut str_written,
    );
    let mut out_b = ValuesMap::default();
    compiled.materialize(&slots, &written, &mut out_b);
    let mut out_str_b = StringValuesMap::default();
    compiled.materialize_str(&str_slots, &str_written, &mut out_str_b);

    assert_eq!(out_a, out_b, "两路径数值输出应一致");
    assert_eq!(out_str_a, out_str_b, "两路径字符串输出应一致");
    // 字符串确实沿槽位流动 (非空转断言): Len("hello") = 5
    assert_eq!(
        out_str_b.get("ti1").and_then(|m| m.get("str")),
        Some(&"hello".to_string())
    );
    assert_eq!(out_b.get("len1").and_then(|m| m.get("result")), Some(&5.0));
}
