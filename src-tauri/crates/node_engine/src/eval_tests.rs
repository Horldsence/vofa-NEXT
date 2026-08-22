//! 数值平面求值测试 — evaluate (慢路径) 与 CompiledEval::run (槽位快路径)

use dsp_filter::{DigitalFilter, FilterKind};
use node_kind::{MathOp, StrNumParams, StrOp};
use node_trigger::TriggerMatchType;

use super::*;
use crate::compile::CompiledGraph;
use crate::test_helpers::*;

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
        &mut HashMap::new(),
        &mut StringValuesMap::default(),
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
        &mut HashMap::new(),
        &mut StringValuesMap::default(),
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
        &mut HashMap::new(),
        &mut StringValuesMap::default(),
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
        &mut HashMap::new(),
        &mut StringValuesMap::default(),
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
        &mut HashMap::new(),
        &mut StringValuesMap::default(),
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
        &mut HashMap::new(),
        &mut StringValuesMap::default(),
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
        &mut HashMap::new(),
        &mut StringValuesMap::default(),
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
        &mut HashMap::new(),
        &mut StringValuesMap::default(),
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
            &mut HashMap::new(),
            &mut StringValuesMap::default(),
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
        &mut HashMap::new(),
        &mut StringValuesMap::default(),
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
        &mut HashMap::new(),
        &mut StringValuesMap::default(),
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
                b: dsp_filter::lowpass_biquad(100.0, 1000.0).0,
                a: dsp_filter::lowpass_biquad(100.0, 1000.0).1,
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
            &mut HashMap::new(),
            &mut StringValuesMap::default(),
        );
        last_y = out
            .get("f1")
            .and_then(|m| m.get("result"))
            .copied()
            .unwrap_or(0.0);
    }
    assert!(
        (last_y - 1.0).abs() < 0.01,
        "低通滤波器直流稳态应接近 1.0, 实际 {last_y}"
    );
}

// ============ ProtocolSource 命名端口 (port_names) 测试 ============

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
    let frames = source_frames(&[("proto1", vec![1.0, 2.0])]); // 第 3 通道越界 → 0

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
    assert_eq!(slots[compiled.slot_of("ps1", "c").unwrap()], 0.0);
}

#[test]
fn test_protocol_source_port_names_fallback() {
    // port_names 越界/空名回退 "ch{i}"; None 保持 ch0..chN (旧前端兼容)
    use node_kind::protocol_source_port_names;
    assert_eq!(protocol_source_port_names(None, 2), vec!["ch0", "ch1"]);
    assert_eq!(protocol_source_port_names(Some(&[]), 2), vec!["ch0", "ch1"]);
    let names = vec!["x".to_string(), String::new()];
    assert_eq!(
        protocol_source_port_names(Some(&names), 3),
        vec!["x", "ch1", "ch2"]
    );
}

// ============ Str 节点测试 (慢路径) ============

#[test]
fn test_str_len_outputs_f32_to_values_map() {
    // Len 输出域为 F32: 写入 ValuesMap, 不写 StringValuesMap;
    // 未连接字符串输入按 "" → 长度 0
    let nodes = vec![make_str("len1", "t1", StrOp::Len)];
    let g = CompiledGraph::compile("t1".into(), nodes, vec![]).unwrap();
    let mut out_str = StringValuesMap::default();
    let out = g.evaluate(
        &empty_frames(),
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
        .ops
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

// ============ Trigger 节点测试 (慢路径) ============

#[test]
fn test_trigger_manual_number_rule_hit() {
    // manual 模式: 每帧以 command 匹配, number 规则命中 → value + matched (text 不写)
    let nodes = vec![make_trigger(
        "tr1",
        "t1",
        "manual",
        "level",
        "GET_TEMP",
        vec![trigger_rule(
            "r1",
            TriggerMatchType::Exact,
            "GET_TEMP",
            "number",
            42.0,
            "",
        )],
    )];
    let g = CompiledGraph::compile("t1".into(), nodes, vec![]).unwrap();
    // 编译期槽位: value/matched 为 f32 槽位, text 为字符串槽位
    assert!(g.compiled().slot_of("tr1", "value").is_some());
    assert!(g.compiled().slot_of("tr1", "matched").is_some());
    assert!(g.compiled().str_slot_of("tr1", "text").is_some());

    let mut out_str = StringValuesMap::default();
    let out = g.evaluate(
        &empty_frames(),
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut out_str,
    );
    assert_eq!(out.get("tr1").and_then(|m| m.get("value")), Some(&42.0));
    assert_eq!(out.get("tr1").and_then(|m| m.get("matched")), Some(&1.0));
    // number 命中不写 text (对齐前端 runMatch 分派)
    assert!(!out_str.contains_key("tr1"));
}

#[test]
fn test_trigger_manual_string_rule_hit_routes_text() {
    // string 规则命中 → text 进 StringValuesMap + matched 写数值平面 (value 不覆盖)
    let nodes = vec![make_trigger(
        "tr1",
        "t1",
        "manual",
        "level",
        "HELLO",
        vec![trigger_rule(
            "r1",
            TriggerMatchType::Exact,
            "HELLO",
            "string",
            0.0,
            "world",
        )],
    )];
    let g = CompiledGraph::compile("t1".into(), nodes, vec![]).unwrap();
    let mut out_str = StringValuesMap::default();
    let out = g.evaluate(
        &empty_frames(),
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut out_str,
    );
    assert_eq!(
        out_str.get("tr1").and_then(|m| m.get("text")),
        Some(&"world".to_string())
    );
    assert_eq!(out.get("tr1").and_then(|m| m.get("matched")), Some(&1.0));
    assert!(
        out.get("tr1").and_then(|m| m.get("value")).is_none(),
        "string 命中不写 value (对齐前端 runMatch)"
    );
}

#[test]
fn test_trigger_manual_miss_defaults() {
    // 未命中 → value = default_miss (-1) + matched = 0;
    // text 不写 (前端 miss 走 number 分支, 不提交 text — 保持上次值)
    let nodes = vec![make_trigger(
        "tr1",
        "t1",
        "manual",
        "level",
        "NOPE",
        vec![trigger_rule(
            "r1",
            TriggerMatchType::Exact,
            "HELLO",
            "number",
            1.0,
            "",
        )],
    )];
    let g = CompiledGraph::compile("t1".into(), nodes, vec![]).unwrap();
    let mut out_str = StringValuesMap::default();
    let out = g.evaluate(
        &empty_frames(),
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut out_str,
    );
    assert_eq!(out.get("tr1").and_then(|m| m.get("value")), Some(&-1.0));
    assert_eq!(out.get("tr1").and_then(|m| m.get("matched")), Some(&0.0));
    assert!(!out_str.contains_key("tr1"));
}

#[test]
fn test_trigger_auto_level_matches_every_active_frame() {
    // auto + level: trigger 非零期间每帧匹配 (Range 规则用数值本身)
    let nodes = vec![
        make_input("knob1", "t1"),
        make_trigger(
            "tr1",
            "t1",
            "auto",
            "level",
            "",
            vec![trigger_rule(
                "r1",
                TriggerMatchType::Range,
                "1..10",
                "number",
                7.0,
                "",
            )],
        ),
    ];
    let edges = vec![edge("e1", "knob1", "value", "tr1", "trigger")];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let mut input_values = HashMap::new();
    let mut trigger_states = HashMap::new();

    let mut eval_with = |v: f32, ts: &mut HashMap<String, node_trigger::TriggerState>| {
        input_values.clear();
        input_values.insert("knob1".to_string(), v);
        let mut out_str = StringValuesMap::default();
        let out = g.evaluate(
            &empty_frames(),
            &input_values,
            &HashMap::new(),
            &mut HashMap::new(),
            &HashMap::new(),
            &mut HashMap::new(),
            ts,
            &mut out_str,
        );
        out.get("tr1")
            .map(|m| (*m.get("value").unwrap(), *m.get("matched").unwrap()))
    };

    assert_eq!(eval_with(0.0, &mut trigger_states), None); // 0 → 不激活
    assert_eq!(eval_with(5.0, &mut trigger_states), Some((7.0, 1.0)));
    assert_eq!(eval_with(5.0, &mut trigger_states), Some((7.0, 1.0))); // level 持续触发
    assert_eq!(eval_with(50.0, &mut trigger_states), Some((-1.0, 0.0))); // 出界 → miss
}

#[test]
fn test_trigger_auto_rising_fires_once() {
    // auto + rising: 仅 0 → 正 上升沿匹配一次, 回落后再升重新触发
    let nodes = vec![
        make_input("knob1", "t1"),
        make_trigger(
            "tr1",
            "t1",
            "auto",
            "rising",
            "",
            vec![trigger_rule(
                "r1",
                TriggerMatchType::Range,
                "1..10",
                "number",
                7.0,
                "",
            )],
        ),
    ];
    let edges = vec![edge("e1", "knob1", "value", "tr1", "trigger")];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let mut input_values = HashMap::new();
    let mut trigger_states = HashMap::new();

    let mut eval_with = |v: f32, ts: &mut HashMap<String, node_trigger::TriggerState>| {
        input_values.clear();
        input_values.insert("knob1".to_string(), v);
        let mut out_str = StringValuesMap::default();
        g.evaluate(
            &empty_frames(),
            &input_values,
            &HashMap::new(),
            &mut HashMap::new(),
            &HashMap::new(),
            &mut HashMap::new(),
            ts,
            &mut out_str,
        )
        .get("tr1")
        .map(|m| (*m.get("value").unwrap(), *m.get("matched").unwrap()))
    };

    assert_eq!(eval_with(5.0, &mut trigger_states), Some((7.0, 1.0))); // 上升沿
    assert_eq!(eval_with(5.0, &mut trigger_states), None); // 持续高位不再触发
    assert_eq!(eval_with(0.0, &mut trigger_states), None);
    assert_eq!(eval_with(5.0, &mut trigger_states), Some((7.0, 1.0))); // 再升再触发
}

#[test]
fn test_trigger_text_flows_through_str_chain() {
    // 任务 2 缺口用例: Trigger(string 规则, 真实文本) → Str(Mid) → Str(Upper)
    // 非空文本沿 string 边流动 (Trigger.text 已有字符串槽位)
    let nodes = vec![
        make_trigger(
            "tr1",
            "t1",
            "manual",
            "level",
            "GO",
            vec![trigger_rule(
                "r1",
                TriggerMatchType::Exact,
                "GO",
                "string",
                0.0,
                "hello world",
            )],
        ),
        make_str_num(
            "mid1",
            "t1",
            StrOp::Mid,
            StrNumParams {
                pos: 0.0,
                len: 5.0,
                size: 0.0,
            },
        ),
        make_str("up1", "t1", StrOp::Upper),
    ];
    let edges = vec![
        edge("e1", "tr1", "text", "mid1", "str"),
        edge("e2", "mid1", "result", "up1", "str"),
    ];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let mut out_str = StringValuesMap::default();
    g.evaluate(
        &empty_frames(),
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
        Some(&"hello".to_string())
    );
    let up = out_str.get("up1").and_then(|m| m.get("result")).cloned();
    assert_eq!(up, Some("HELLO".to_string()));
    assert!(!up.unwrap().is_empty(), "文本应非空沿 string 边流动");
}

#[test]
fn test_trigger_value_feeds_str_num_port_via_math() {
    // 任务 2 缺口用例: Str 数值端口已连接时走上游值 (Trigger.value → Math → Mid.pos)
    // pos=2 (上游) 时 Mid("hello", 2, 2) = "el" (1-based, 见 StrOp::Mid 测试);
    // 若误用内联回退 pos=9 则越界得 ""
    let nodes = vec![
        make_trigger(
            "tr_num",
            "t1",
            "manual",
            "level",
            "GO",
            vec![trigger_rule(
                "r1",
                TriggerMatchType::Exact,
                "GO",
                "number",
                2.0,
                "",
            )],
        ),
        make_trigger(
            "tr_text",
            "t1",
            "manual",
            "level",
            "IN",
            vec![trigger_rule(
                "r2",
                TriggerMatchType::Exact,
                "IN",
                "string",
                0.0,
                "hello",
            )],
        ),
        make_math("m1", "t1", MathOp::Abs, 1),
        make_str_num(
            "mid1",
            "t1",
            StrOp::Mid,
            StrNumParams {
                pos: 9.0,
                len: 2.0,
                size: 0.0,
            },
        ),
    ];
    let edges = vec![
        edge("e1", "tr_num", "value", "m1", "in0"),
        edge("e2", "m1", "result", "mid1", "pos"),
        edge("e3", "tr_text", "text", "mid1", "str"),
    ];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let mut out_str = StringValuesMap::default();
    g.evaluate(
        &empty_frames(),
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
        Some(&"el".to_string()),
        "pos 应用上游值 2.0 (而非内联回退 9.0)"
    );
}

#[test]
fn test_trigger_manual_tracks_prev_no_false_rising_on_mode_switch() {
    // 对齐前端 useEffect: 非 auto 模式仍每帧跟踪 prevTriggerRef。
    // manual 期间 trigger 输入 0→5; 切回 auto+rising (图重编译, 配置仅 mode 变化
    // → 不重建 TriggerState, prev 保留) 且输入保持 5 → 不应误触发上升沿。
    let rules = || {
        vec![trigger_rule(
            "r1",
            TriggerMatchType::Range,
            "1..10",
            "number",
            7.0,
            "",
        )]
    };
    let edges = || vec![edge("e1", "knob1", "value", "tr1", "trigger")];
    let g_manual = CompiledGraph::compile(
        "t1".into(),
        vec![
            make_input("knob1", "t1"),
            make_trigger("tr1", "t1", "manual", "rising", "GO", rules()),
        ],
        edges(),
    )
    .unwrap();
    let mut input_values = HashMap::new();
    let mut trigger_states = HashMap::new();

    let mut eval_with =
        |g: &CompiledGraph, v: f32, ts: &mut HashMap<String, node_trigger::TriggerState>| {
            input_values.clear();
            input_values.insert("knob1".to_string(), v);
            let mut out_str = StringValuesMap::default();
            g.evaluate(
                &empty_frames(),
                &input_values,
                &HashMap::new(),
                &mut HashMap::new(),
                &HashMap::new(),
                &mut HashMap::new(),
                ts,
                &mut out_str,
            )
            .get("tr1")
            .map(|m| (*m.get("value").unwrap(), *m.get("matched").unwrap()))
        };

    // manual: 输入 0 → 5, prev 被跟踪 (command "GO" 不匹配 Range, miss)
    assert_eq!(
        eval_with(&g_manual, 0.0, &mut trigger_states),
        Some((-1.0, 0.0))
    );
    assert_eq!(
        eval_with(&g_manual, 5.0, &mut trigger_states),
        Some((-1.0, 0.0))
    );

    // 切回 auto+rising (仅 mode 变化, TriggerState 不重建), 输入保持 5
    let g_auto = CompiledGraph::compile(
        "t1".into(),
        vec![
            make_input("knob1", "t1"),
            make_trigger("tr1", "t1", "auto", "rising", "GO", rules()),
        ],
        edges(),
    )
    .unwrap();
    assert_eq!(
        eval_with(&g_auto, 5.0, &mut trigger_states),
        None,
        "prev 已在 manual 期间跟踪为 5, 不应误触发上升沿"
    );
    // 回落到 0 后再升: 正常触发
    assert_eq!(eval_with(&g_auto, 0.0, &mut trigger_states), None);
    assert_eq!(
        eval_with(&g_auto, 5.0, &mut trigger_states),
        Some((7.0, 1.0))
    );
}

// ============ TextInput 节点测试 ============

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
