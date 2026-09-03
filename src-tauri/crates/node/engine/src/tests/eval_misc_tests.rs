//! ProtocolSource 等杂项求值测试
#![allow(unused_imports, dead_code)]

use dsp_filter::{DigitalFilter, FilterConfig};
use kind::{MathOp, StrNumParams, StrOp};
use trigger::TriggerMatchType;

use super::*;
use crate::compile::CompiledGraph;
use testkit::*;

#[test]
fn test_evaluate_protocol_source() {
    let nodes = vec![make_protocol_source("ps1", "t1", "proto1", 2)];
    let g = CompiledGraph::compile("t1".into(), nodes, vec![]).unwrap();
    let frames = source_frames(&[("proto1", vec![10.0, 20.0])]);
    let out = g.evaluate(
        &frames,
        &empty_texts(),
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut StringValuesMap::default(),
    );
    assert_eq!(out.get("ps1").and_then(|m| m.get("ch0")), Some(&10.0));
    assert_eq!(out.get("ps1").and_then(|m| m.get("ch1")), Some(&20.0));
}

#[test]
fn test_protocol_source_multi_source() {
    // 多协议源并存: 每个 ProtocolSource 从自己的源读最新帧
    let nodes = vec![
        make_protocol_source("ps_a", "t1", "proto_a", 1),
        make_protocol_source("ps_b", "t1", "proto_b", 1),
        make_math("m1", "t1", MathOp::Add, 2),
    ];
    let edges = vec![
        edge("e1", "ps_a", "ch0", "m1", "in0"),
        edge("e2", "ps_b", "ch0", "m1", "in1"),
    ];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let frames = source_frames(&[("proto_a", vec![3.0]), ("proto_b", vec![4.0])]);
    let out = g.evaluate(
        &frames,
        &empty_texts(),
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut StringValuesMap::default(),
    );
    assert_eq!(out.get("ps_a").and_then(|m| m.get("ch0")), Some(&3.0));
    assert_eq!(out.get("ps_b").and_then(|m| m.get("ch0")), Some(&4.0));
    assert_eq!(out.get("m1").and_then(|m| m.get("result")), Some(&7.0));
}

#[test]
fn test_protocol_source_missing_source_is_not_materialized() {
    // 源缺失 / 通道越界 → 不写；不得与真实 0.0 混淆。
    let nodes = vec![make_protocol_source("ps1", "t1", "proto_missing", 3)];
    let g = CompiledGraph::compile("t1".into(), nodes, vec![]).unwrap();
    // 完全缺源
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
    assert!(out.get("ps1").and_then(|m| m.get("ch0")).is_none());
    // 源存在但通道数不足 → 只物化真实存在的通道
    let frames = source_frames(&[("proto_missing", vec![9.0])]);
    let out = g.evaluate(
        &frames,
        &empty_texts(),
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut StringValuesMap::default(),
    );
    assert_eq!(out.get("ps1").and_then(|m| m.get("ch0")), Some(&9.0));
    assert!(out.get("ps1").and_then(|m| m.get("ch2")).is_none());
}

#[test]
fn test_protocol_source_named_ports_evaluate() {
    // 命名端口: channels[i] 写入第 i 个命名槽位 (慢路径)
    let nodes = vec![
        make_protocol_source_named("ps1", "t1", "proto1", &["temp", "humi"]),
        make_math("m1", "t1", MathOp::Add, 2),
    ];
    let edges = vec![
        edge("e1", "ps1", "temp", "m1", "in0"),
        edge("e2", "ps1", "humi", "m1", "in1"),
    ];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let frames = source_frames(&[("proto1", vec![36.5, 60.0])]);
    let out = g.evaluate(
        &frames,
        &empty_texts(),
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut StringValuesMap::default(),
    );
    assert_eq!(out.get("ps1").and_then(|m| m.get("temp")), Some(&36.5));
    assert_eq!(out.get("ps1").and_then(|m| m.get("humi")), Some(&60.0));
    // 命名端口下不应再有 ch0/ch1
    assert!(out.get("ps1").and_then(|m| m.get("ch0")).is_none());
    // 命名端口参与下游求值
    assert_eq!(out.get("m1").and_then(|m| m.get("result")), Some(&96.5));
}

#[test]
#[allow(clippy::float_cmp)] // 通道值原样写入槽位, 为精确可表示的小整数
fn test_protocol_source_named_ports_slot_run() {
    // 命名端口: 槽位快路径 (CompiledEval::run) 与慢路径语义一致
    let nodes = vec![make_protocol_source_named(
        "ps1",
        "t1",
        "proto1",
        &["a", "b", "c"],
    )];
    let g = CompiledGraph::compile("t1".into(), nodes, vec![]).unwrap();
    let frames = source_frames(&[("proto1", vec![1.0, 2.0])]); // 第 3 通道越界 → 未写

    // 槽位名检查: 应分配 a/b/c 三个命名槽位
    let compiled = g.compiled();
    assert!(compiled.slot_of("ps1", "a").is_some());
    assert!(compiled.slot_of("ps1", "b").is_some());
    assert!(compiled.slot_of("ps1", "c").is_some());
    assert!(compiled.slot_of("ps1", "ch0").is_none());

    let mut slots = vec![0.0f32; compiled.slot_count()];
    let mut written = vec![false; compiled.slot_count()];
    compiled.run(
        &frames,
        &empty_texts(),
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut slots,
        &mut written,
        &mut [],
        &mut [],
    );
    assert_eq!(slots[compiled.slot_of("ps1", "a").unwrap()], 1.0);
    assert_eq!(slots[compiled.slot_of("ps1", "b").unwrap()], 2.0);
    assert!(!written[compiled.slot_of("ps1", "c").unwrap()]);
}

#[test]
fn test_protocol_source_port_names_fallback() {
    // port_names 越界/空名回退 "ch{i}"; None 保持 ch0..chN (旧前端兼容)
    use kind::protocol_source_port_names;
    assert_eq!(protocol_source_port_names(None, 2), vec!["ch0", "ch1"]);
    assert_eq!(protocol_source_port_names(Some(&[]), 2), vec!["ch0", "ch1"]);
    let names = vec!["x".to_string(), String::new()];
    assert_eq!(
        protocol_source_port_names(Some(&names), 3),
        vec!["x", "ch1", "ch2"]
    );
}

#[test]
fn test_protocol_source_str_port_slow_path() {
    // port_names 含 "str" (String 域): 跳过 F32 写入, 改从 source_texts 读值写 out_str;
    // 混合命名端口 (temp F32 + str String) 互不影响 (通道下标按端口位次独立)
    let nodes = vec![make_protocol_source_named(
        "ps1",
        "t1",
        "proto1",
        &["temp", "str"],
    )];
    let g = CompiledGraph::compile("t1".into(), nodes, vec![]).unwrap();
    let frames = source_frames(&[("proto1", vec![36.5])]);
    let texts = source_texts(&[("proto1", "hello")]);
    let mut out_str = StringValuesMap::default();
    let out = g.evaluate(
        &frames,
        &texts,
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut out_str,
    );
    assert_eq!(out.get("ps1").and_then(|m| m.get("temp")), Some(&36.5));
    // "str" 不写数值平面
    assert!(out.get("ps1").and_then(|m| m.get("str")).is_none());
    assert_eq!(
        out_str.get("ps1").and_then(|m| m.get("str")),
        Some(&"hello".to_string())
    );

    // 源无缓存文本: "str" 不写 (保持上次值, 对齐 Trigger 未激活帧语义) — out_str 无该键
    let mut out_str = StringValuesMap::default();
    g.evaluate(
        &frames,
        &empty_texts(),
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut out_str,
    );
    assert!(!out_str.contains_key("ps1"));
}

#[test]
fn test_protocol_source_str_port_slot_run() {
    // 快路径 (CompiledEval::run + materialize_str) 与慢路径同语义:
    // "str" 分配字符串槽位 (不占数值槽位), run 时从 source_texts 写;
    // 无数据时 str_written 不置位 → materialize_str 无键 (快照保持上次值)
    let nodes = vec![make_protocol_source_named("ps1", "t1", "proto1", &["str"])];
    let g = CompiledGraph::compile("t1".into(), nodes, vec![]).unwrap();
    let compiled = g.compiled();
    assert!(compiled.str_slot_of("ps1", "str").is_some());
    assert!(compiled.slot_of("ps1", "str").is_none());

    let mut slots = vec![0.0f32; compiled.slot_count()];
    let mut written = vec![false; compiled.slot_count()];
    let mut str_slots = vec![String::new(); compiled.str_slot_count()];
    let mut str_written = vec![false; compiled.str_slot_count()];

    let texts = source_texts(&[("proto1", "abc")]);
    compiled.run(
        &empty_frames(),
        &texts,
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
    let mut out_str = StringValuesMap::default();
    compiled.materialize_str(&str_slots, &str_written, &mut out_str);
    assert_eq!(
        out_str.get("ps1").and_then(|m| m.get("str")),
        Some(&"abc".to_string())
    );

    // 无数据帧: 槽位清零后重跑 (模拟跨帧), str_written 不置位 → 无键
    str_slots.iter_mut().for_each(String::clear);
    str_written.fill(false);
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
    let mut out_str = StringValuesMap::default();
    compiled.materialize_str(&str_slots, &str_written, &mut out_str);
    assert!(!out_str.contains_key("ps1"));
}
