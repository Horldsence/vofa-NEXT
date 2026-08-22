//! 数值平面求值测试 — evaluate (慢路径) 与 CompiledEval::run (槽位快路径)

use dsp_filter::{DigitalFilter, FilterKind};
use node_kind::{MathOp, StrNumParams, StrOp};

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
