//! 数值平面求值测试 — evaluate (慢路径) 与 CompiledEval::run (槽位快路径)

use std::collections::HashMap;

use super::*;
use crate::compile::CompiledGraph;
use crate::test_helpers::*;
use crate::FilterKind;

fn empty_frames() -> SourceFramesMap {
    SourceFramesMap::default()
}

#[test]
fn test_evaluate_protocol_source() {
    let nodes = vec![make_protocol_source("ps1", "t1", "proto1", 2)];
    let g = CompiledGraph::compile("t1".into(), nodes, vec![]).unwrap();
    let frames = source_frames(&[("proto1", vec![10.0, 20.0])]);
    let out = g.evaluate(
        &frames,
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
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
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
    );
    assert_eq!(out.get("ps_a").and_then(|m| m.get("ch0")), Some(&3.0));
    assert_eq!(out.get("ps_b").and_then(|m| m.get("ch0")), Some(&4.0));
    assert_eq!(out.get("m1").and_then(|m| m.get("result")), Some(&7.0));
}

#[test]
fn test_protocol_source_missing_source_writes_zero() {
    // 源缺失 / 通道越界 → 写 0.0 (与未连接语义一致)
    let nodes = vec![make_protocol_source("ps1", "t1", "proto_missing", 3)];
    let g = CompiledGraph::compile("t1".into(), nodes, vec![]).unwrap();
    // 完全缺源
    let out = g.evaluate(
        &empty_frames(),
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
    );
    assert_eq!(out.get("ps1").and_then(|m| m.get("ch0")), Some(&0.0));
    // 源存在但通道数不足 → 越界通道 0.0
    let frames = source_frames(&[("proto_missing", vec![9.0])]);
    let out = g.evaluate(
        &frames,
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
    );
    assert_eq!(out.get("ps1").and_then(|m| m.get("ch0")), Some(&9.0));
    assert_eq!(out.get("ps1").and_then(|m| m.get("ch2")), Some(&0.0));
}

#[test]
fn test_evaluate_input_node() {
    let nodes = vec![make_input("knob1", "t1")];
    let g = CompiledGraph::compile("t1".into(), nodes, vec![]).unwrap();
    let mut input_values = HashMap::new();
    input_values.insert("knob1".to_string(), 42.0_f32);
    let out = g.evaluate(
        &empty_frames(),
        &input_values,
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
    );
    assert_eq!(out.get("knob1").and_then(|m| m.get("value")), Some(&42.0));
}

#[test]
fn test_evaluate_math_add() {
    let nodes = vec![
        make_protocol_source("ps1", "t1", "proto1", 2),
        make_math("m1", "t1", MathOp::Add, 2),
    ];
    let edges = vec![
        edge("e1", "ps1", "ch0", "m1", "in0"),
        edge("e2", "ps1", "ch1", "m1", "in1"),
    ];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let frames = source_frames(&[("proto1", vec![10.0, 20.0])]);
    let out = g.evaluate(
        &frames,
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
    );
    // m1.result = 10 + 20 = 30
    assert_eq!(out.get("m1").and_then(|m| m.get("result")), Some(&30.0));
}

#[test]
fn test_evaluate_math_chain() {
    // m1 = ch0 + ch1, m2 = m1 * m1
    let nodes = vec![
        make_protocol_source("ps1", "t1", "proto1", 2),
        make_math("m1", "t1", MathOp::Add, 2),
        make_math("m2", "t1", MathOp::Mul, 2),
    ];
    let edges = vec![
        edge("e1", "ps1", "ch0", "m1", "in0"),
        edge("e2", "ps1", "ch1", "m1", "in1"),
        edge("e3", "m1", "result", "m2", "in0"),
        edge("e4", "m1", "result", "m2", "in1"),
    ];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let frames = source_frames(&[("proto1", vec![3.0, 4.0])]);
    let out = g.evaluate(
        &frames,
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
    );
    // m1 = 3 + 4 = 7, m2 = 7 * 7 = 49
    assert_eq!(out.get("m1").and_then(|m| m.get("result")), Some(&7.0));
    assert_eq!(out.get("m2").and_then(|m| m.get("result")), Some(&49.0));
}

#[test]
fn test_evaluate_custom_node() {
    let nodes = vec![
        make_protocol_source("ps1", "t1", "proto1", 1),
        make_custom("c1", "t1", vec!["value"], vec!["out"]),
    ];
    let edges = vec![edge("e1", "ps1", "ch0", "c1", "value")];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let frames = source_frames(&[("proto1", vec![5.0])]);
    let mut custom_outputs: HashMap<String, HashMap<String, f32>> = HashMap::new();
    let mut m = HashMap::new();
    m.insert("out".to_string(), 99.0);
    custom_outputs.insert("c1".to_string(), m);

    let out = g.evaluate(
        &frames,
        &HashMap::new(),
        &custom_outputs,
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
    );
    assert_eq!(out.get("c1").and_then(|m| m.get("out")), Some(&99.0));

    // collect_custom_inputs 应返回 c1.value = 5.0
    let custom_inputs = g.collect_custom_inputs(&out);
    assert_eq!(
        custom_inputs.get("c1").and_then(|m| m.get("value")),
        Some(&5.0)
    );
}

#[test]
fn test_unary_math() {
    let nodes = vec![
        make_protocol_source("ps1", "t1", "proto1", 1),
        make_math("m1", "t1", MathOp::Abs, 1),
    ];
    let edges = vec![edge("e1", "ps1", "ch0", "m1", "in0")];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let frames = source_frames(&[("proto1", vec![-5.0])]);
    let out = g.evaluate(
        &frames,
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
    );
    assert_eq!(out.get("m1").and_then(|m| m.get("result")), Some(&5.0));
}

// ============ Filter 节点测试 ============

#[test]
fn test_filter_fir_passthrough() {
    // FIR b=[1.0] → 通过 (y = x)
    let nodes = vec![
        make_protocol_source("ps1", "t1", "proto1", 1),
        make_filter("f1", "t1", FilterKind::FIR { b: vec![1.0] }),
    ];
    let edges = vec![edge("e1", "ps1", "ch0", "f1", "in0")];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let frames = source_frames(&[("proto1", vec![7.5])]);
    let mut filter_states = HashMap::new();
    let out = g.evaluate(
        &frames,
        &HashMap::new(),
        &HashMap::new(),
        &mut filter_states,
        &HashMap::new(),
        &mut HashMap::new(),
    );
    assert_eq!(out.get("f1").and_then(|m| m.get("result")), Some(&7.5));
    // filter_states 应包含 f1
    assert!(filter_states.contains_key("f1"));
}

#[test]
fn test_filter_fir_delay_state_persistence() {
    // FIR b=[0.0, 1.0] → 延迟一拍 (y[n] = x[n-1])
    // 验证 filter_states 跨帧持久化
    let nodes = vec![
        make_protocol_source("ps1", "t1", "proto1", 1),
        make_filter("f1", "t1", FilterKind::FIR { b: vec![0.0, 1.0] }),
    ];
    let edges = vec![edge("e1", "ps1", "ch0", "f1", "in0")];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let mut filter_states = HashMap::new();

    let eval_with = |x: f32, fs: &mut HashMap<String, DigitalFilter>| {
        let frames = source_frames(&[("proto1", vec![x])]);
        let out = g.evaluate(
            &frames,
            &HashMap::new(),
            &HashMap::new(),
            fs,
            &HashMap::new(),
            &mut HashMap::new(),
        );
        out.get("f1").and_then(|m| m.get("result")).copied()
    };

    // 帧 1: x=1.0, y=0.0 (x[-1]=0)
    assert_eq!(eval_with(1.0, &mut filter_states), Some(0.0));
    // 帧 2: x=2.0, y=1.0 (x[0]=1, 状态持久化生效)
    assert_eq!(eval_with(2.0, &mut filter_states), Some(1.0));
    // 帧 3: x=3.0, y=2.0
    assert_eq!(eval_with(3.0, &mut filter_states), Some(2.0));
}

#[test]
fn test_filter_kind_change_rebuilds_state() {
    // 用户修改 Filter 配置时, 状态应重建
    // 初始: FIR b=[1.0] (通过)
    let nodes = vec![
        make_protocol_source("ps1", "t1", "proto1", 1),
        make_filter("f1", "t1", FilterKind::FIR { b: vec![1.0] }),
    ];
    let edges = vec![edge("e1", "ps1", "ch0", "f1", "in0")];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let mut filter_states = HashMap::new();

    // 帧 1: 通过, y=5.0
    let frames = source_frames(&[("proto1", vec![5.0])]);
    let _ = g.evaluate(
        &frames,
        &HashMap::new(),
        &HashMap::new(),
        &mut filter_states,
        &HashMap::new(),
        &mut HashMap::new(),
    );
    assert!(filter_states.contains_key("f1"));

    // 重新编译图: 修改 Filter kind 为 b=[2.0] (放大 2 倍)
    let nodes2 = vec![
        make_protocol_source("ps1", "t1", "proto1", 1),
        make_filter("f1", "t1", FilterKind::FIR { b: vec![2.0] }),
    ];
    let edges2 = vec![edge("e1", "ps1", "ch0", "f1", "in0")];
    let g2 = CompiledGraph::compile("t1".into(), nodes2, edges2).unwrap();
    // 帧 2: 新 kind, 应重建状态, y = 2.0 * 3.0 = 6.0
    let frames2 = source_frames(&[("proto1", vec![3.0])]);
    let out2 = g2.evaluate(
        &frames2,
        &HashMap::new(),
        &HashMap::new(),
        &mut filter_states,
        &HashMap::new(),
        &mut HashMap::new(),
    );
    assert_eq!(out2.get("f1").and_then(|m| m.get("result")), Some(&6.0));
}

#[test]
fn test_filter_lowpass_preserves_dc() {
    // 低通滤波器对直流信号 (常数) 应基本保持原值
    let nodes = vec![
        make_protocol_source("ps1", "t1", "proto1", 1),
        make_filter(
            "f1",
            "t1",
            FilterKind::IIR {
                b: vofa_next_dsp::filter::lowpass_biquad(100.0, 1000.0).0,
                a: vofa_next_dsp::filter::lowpass_biquad(100.0, 1000.0).1,
            },
        ),
    ];
    let edges = vec![edge("e1", "ps1", "ch0", "f1", "in0")];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let mut filter_states = HashMap::new();

    // 连续输入 1.0 (直流), 稳态后应接近 1.0
    let mut last_y = 0.0;
    for _ in 0..200 {
        let frames = source_frames(&[("proto1", vec![1.0])]);
        let out = g.evaluate(
            &frames,
            &HashMap::new(),
            &HashMap::new(),
            &mut filter_states,
            &HashMap::new(),
            &mut HashMap::new(),
        );
        last_y = out
            .get("f1")
            .and_then(|m| m.get("result"))
            .copied()
            .unwrap_or(0.0);
    }
    assert!(
        (last_y - 1.0).abs() < 0.01,
        "低通滤波器直流稳态应接近 1.0, 实际 {}",
        last_y
    );
}
