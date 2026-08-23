//! 槽位等价性 / SpectrumSink / Ifft 求值测试

use std::collections::HashMap;

use dsp_fft::{IfftState, SpectrumOutput};
use dsp_filter::FilterConfig;
use dsp_window::WindowType;
use node_kind::{DecoderBlockDef, MathOp, NodeDef, NodeKind};

use super::SourceFramesMap;
use crate::compile::CompiledGraph;
use crate::test_helpers::*;
use crate::{StringValuesMap, ValuesMap};

fn empty_frames() -> SourceFramesMap {
    SourceFramesMap::default()
}

// ============ SpectrumSink 节点测试 ============

#[test]
fn test_collect_spectrum_inputs() {
    // collect_spectrum_inputs 应返回 SpectrumSink 的输入值
    let nodes = vec![
        make_protocol_source("ps1", "t1", "proto1", 1),
        make_spectrum_sink(
            "s1",
            "t1",
            256,
            WindowType::Hann,
            SpectrumOutput::Magnitude,
            1000.0,
        ),
    ];
    let edges = vec![edge("e1", "ps1", "ch0", "s1", "in0")];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let frames = source_frames(&[("proto1", vec![42.0])]);
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

    // collect_spectrum_inputs 应返回 s1 → 42.0
    let spectrum_inputs = g.collect_spectrum_inputs(&out);
    assert_eq!(spectrum_inputs.get("s1"), Some(&42.0));
}

#[test]
fn test_spectrum_sink_no_output_in_evaluate() {
    // evaluate 不应包含 SpectrumSink 的输出
    let nodes = vec![
        make_protocol_source("ps1", "t1", "proto1", 1),
        make_spectrum_sink(
            "s1",
            "t1",
            256,
            WindowType::Hann,
            SpectrumOutput::Magnitude,
            1000.0,
        ),
    ];
    let edges = vec![edge("e1", "ps1", "ch0", "s1", "in0")];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let frames = source_frames(&[("proto1", vec![1.0])]);
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
    // s1 不应在 evaluate 输出中
    assert!(!out.contains_key("s1"));
    // 但 ProtocolSource 应在
    assert!(out.contains_key("ps1"));
}

// ============ Ifft 节点测试 ============

#[test]
fn test_ifft_node_reads_playback_buffer() {
    // Ifft 节点输出应从 ifft_states 环形读取重建缓冲
    let nodes = vec![NodeDef {
        id: "ifft1".to_string(),
        tab_id: "t1".to_string(),
        kind: NodeKind::Ifft,
    }];
    let g = CompiledGraph::compile("t1".into(), nodes, vec![]).unwrap();

    let mut ifft_states: HashMap<String, IfftState> = HashMap::new();
    let mut st = IfftState::default();
    // DC 振幅谱: bin0=1, 其余 0 → 重建为常数 1 (n=8)
    let n = 8;
    let magnitudes: Vec<f32> = {
        let mut v = vec![0.0f32; n / 2 + 1];
        v[0] = 1.0;
        v
    };
    st.synth(&magnitudes, n);
    ifft_states.insert("ifft1".to_string(), st);

    // 环形播放应持续输出 1.0
    for _ in 0..(n * 3) {
        let out = g.evaluate(
            &empty_frames(),
            &empty_texts(),
            &HashMap::new(),
            &HashMap::new(),
            &mut HashMap::new(),
            &HashMap::new(),
            &mut ifft_states,
            &mut HashMap::new(),
            &mut StringValuesMap::default(),
        );
        assert_eq!(out.get("ifft1").and_then(|m| m.get("out0")), Some(&1.0));
    }
}

// ============ CompiledEval 槽位评估等价性测试 ============

/// 槽位评估 (compiled.run + materialize) 与 evaluate_into 逐帧完全等价
///
/// 覆盖: ProtocolSource(4ch) / 链式 Math×2 / Filter(FIR, 跨帧状态) / Input /
/// FrameDecoder 无 parser (默认 0 端口) — 100 帧伪随机数据逐帧比对
#[test]
#[allow(clippy::cast_precision_loss, clippy::suboptimal_flops)] // 伪随机输入生成, 精度无关
fn test_compiled_eval_equivalence() {
    let nodes = vec![
        make_protocol_source("ps1", "t1", "proto1", 4),
        make_math("m1", "t1", MathOp::Add, 2),
        make_math("m2", "t1", MathOp::Mul, 2),
        make_filter("f1", "t1", FilterConfig::Lowpass { cutoff: 100.0, sample_rate: 1000.0 }),
        make_input("knob1", "t1"),
        // FrameDecoder 无 parser (decoder_states 为空) — 覆盖 written 语义
        NodeDef {
            id: "d1".to_string(),
            tab_id: "t1".to_string(),
            kind: NodeKind::FrameDecoder {
                blocks: vec![DecoderBlockDef::Field {
                    id: "f".to_string(),
                    field_type: node_kind::FieldType::UInt8,
                    port_name: "value".to_string(),
                    length_ref: None,
                    match_id: None,
                }],
                enable_valid: true,
                enable_frame_count: true,
                enable_last_timestamp: false,
                enable_fps: false,
                loopback: false,
            },
        },
    ];
    let edges = vec![
        edge("e1", "ps1", "ch0", "m1", "in0"),
        edge("e2", "ps1", "ch1", "m1", "in1"),
        edge("e3", "m1", "result", "m2", "in0"),
        edge("e4", "ps1", "ch2", "m2", "in1"),
        edge("e5", "m2", "result", "f1", "in0"),
    ];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();

    let mut input_values = HashMap::new();
    input_values.insert("knob1".to_string(), 7.0_f32);
    let custom_outputs = HashMap::new();
    let decoder_states = HashMap::new(); // d1 无 parser
                                         // 两条路径各自独立的 filter_states (跨帧状态)
    let mut fs_a = HashMap::new();
    let mut fs_b = HashMap::new();

    let compiled = g.compiled();
    let n = compiled.slot_count();
    let mut slots = vec![0.0f32; n];
    let mut written = vec![false; n];
    let mut str_slots = vec![String::new(); compiled.str_slot_count()];
    let mut str_written = vec![false; compiled.str_slot_count()];

    // 确定性伪随机 (LCG)
    let mut seed = 0x12345678u32;
    let mut next_f = move || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 8) as f32 / 16777216.0 * 20.0 - 10.0
    };

    for frame_idx in 0..100 {
        let frames = source_frames(&[("proto1", vec![next_f(), next_f(), next_f(), next_f()])]);
        // 老路径: evaluate_into
        let mut out_a = ValuesMap::default();
        let mut out_str_a = StringValuesMap::default();
        g.evaluate_into(
            &frames,
            &empty_texts(),
            &input_values,
            &custom_outputs,
            &mut fs_a,
            &decoder_states,
            &mut HashMap::new(),
            &mut HashMap::new(),
            &mut out_a,
            &mut out_str_a,
        );
        // 新路径: compiled.run + materialize (每帧清零 slots/written)
        slots.fill(0.0);
        written.fill(false);
        str_slots.iter_mut().for_each(String::clear);
        str_written.fill(false);
        compiled.run(
            &frames,
            &empty_texts(),
            &input_values,
            &custom_outputs,
            &mut fs_b,
            &decoder_states,
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
        assert_eq!(out_a, out_b, "帧 {frame_idx} 输出不一致");
        assert_eq!(out_str_a, out_str_b, "帧 {frame_idx} 字符串输出不一致");
    }

    // FrameDecoder 无 parser: 两边都输出默认 0 端口 (value/valid/frame_count)
    let mut out_a = ValuesMap::default();
    let frames = source_frames(&[("proto1", vec![1.0, 2.0, 3.0, 4.0])]);
    g.evaluate_into(
        &frames,
        &empty_texts(),
        &input_values,
        &custom_outputs,
        &mut fs_a,
        &decoder_states,
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut out_a,
        &mut StringValuesMap::default(),
    );
    let d1 = out_a.get("d1").expect("d1 应输出默认端口");
    assert_eq!(d1.get("value"), Some(&0.0));
    assert_eq!(d1.get("valid"), Some(&0.0));
    assert_eq!(d1.get("frame_count"), Some(&0.0));
    assert!(!d1.contains_key("last_timestamp")); // enable_last_timestamp = false
}

/// 字符串图等价性: 混合链 Str(Mid) → Str(Upper) (字符串平面) +
/// Str(Len) → Math (字符串→数值平面), 覆盖 "未连接数值端口走 num_defaults"
/// 与 "已连接走上游" 两种情形 — 100 帧逐帧断言 ValuesMap 与 StringValuesMap 都一致
#[test]
fn test_compiled_eval_str_equivalence() {
    let nodes = vec![
        make_input("knob1", "t1"),
        // mid: pos 已连接 (knob1), len 未连接 (走 num_defaults 回退 2.0)
        make_str_num(
            "mid1",
            "t1",
            node_kind::StrOp::Mid,
            node_kind::StrNumParams {
                pos: 1.0,
                len: 2.0,
                size: 0.0,
            },
        ),
        make_str("up1", "t1", node_kind::StrOp::Upper),
        make_str("len1", "t1", node_kind::StrOp::Len),
        make_math("m1", "t1", MathOp::Add, 2),
    ];
    let edges = vec![
        edge("e1", "knob1", "value", "mid1", "pos"), // F32: 已连接数值端口
        edge("e2", "mid1", "result", "up1", "str"),  // String → String
        edge("e3", "up1", "result", "len1", "str"),  // String → String
        edge("e4", "len1", "result", "m1", "in0"),   // F32 (Len 输出)
        edge("e5", "knob1", "value", "m1", "in1"),
    ];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();

    let custom_outputs = HashMap::new();
    let decoder_states = HashMap::new();
    let mut fs_a = HashMap::new();
    let mut fs_b = HashMap::new();

    let compiled = g.compiled();
    let mut slots = vec![0.0f32; compiled.slot_count()];
    let mut written = vec![false; compiled.slot_count()];
    let mut str_slots = vec![String::new(); compiled.str_slot_count()];
    let mut str_written = vec![false; compiled.str_slot_count()];

    for frame_idx in 0..100u16 {
        let mut input_values = HashMap::new();
        input_values.insert("knob1".to_string(), f32::from(frame_idx) * 0.5);
        // 老路径: evaluate_into
        let mut out_a = ValuesMap::default();
        let mut out_str_a = StringValuesMap::default();
        g.evaluate_into(
            &empty_frames(),
            &empty_texts(),
            &input_values,
            &custom_outputs,
            &mut fs_a,
            &decoder_states,
            &mut HashMap::new(),
            &mut HashMap::new(),
            &mut out_a,
            &mut out_str_a,
        );
        // 新路径: compiled.run + materialize / materialize_str (每帧清零)
        slots.fill(0.0);
        written.fill(false);
        str_slots.iter_mut().for_each(String::clear);
        str_written.fill(false);
        compiled.run(
            &empty_frames(),
            &empty_texts(),
            &input_values,
            &custom_outputs,
            &mut fs_b,
            &decoder_states,
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
        assert_eq!(out_a, out_b, "帧 {frame_idx} 数值输出不一致");
        assert_eq!(out_str_a, out_str_b, "帧 {frame_idx} 字符串输出不一致");
        // 字符串平面确实参与了求值 (非空 map 保证测试不是空转)
        assert!(out_str_a.contains_key("mid1"));
        assert!(out_str_a.contains_key("up1"));
    }
}

/// Trigger 等价性: auto rising (number 规则) + auto level (string 规则 → Str 链)
/// + manual (string 规则) 混合图 — 100 帧逐帧断言 ValuesMap + StringValuesMap 一致
///   (trigger_states 两份独立状态, 仿 fs_a/fs_b 模式; 覆盖"未激活帧不写槽位"语义)
#[test]
#[allow(clippy::cast_precision_loss)] // 帧号转 f32 构造确定性输入, 精度无关
fn test_compiled_eval_trigger_equivalence() {
    use node_trigger::{TriggerMatchType, TriggerState};

    let nodes = vec![
        make_protocol_source("ps1", "t1", "proto1", 2),
        // auto rising: ch0 在 0/5 间交替 (5 帧高分组) → 每组只触发一次
        make_trigger(
            "tr_rise",
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
        // auto level: ch1 恒在 20..22 → string 规则每帧命中
        make_trigger(
            "tr_lvl",
            "t1",
            "auto",
            "level",
            "",
            vec![trigger_rule(
                "r2",
                TriggerMatchType::Range,
                "20..30",
                "string",
                0.0,
                "HI",
            )],
        ),
        // manual: 恒定 string 命中
        make_trigger(
            "tr_man",
            "t1",
            "manual",
            "level",
            "GO",
            vec![trigger_rule(
                "r3",
                TriggerMatchType::Exact,
                "GO",
                "string",
                0.0,
                "ok",
            )],
        ),
        make_str("up1", "t1", node_kind::StrOp::Upper),
        make_math("m1", "t1", MathOp::Add, 2),
    ];
    let edges = vec![
        edge("e1", "ps1", "ch0", "tr_rise", "trigger"),
        edge("e2", "ps1", "ch1", "tr_lvl", "trigger"),
        edge("e3", "tr_lvl", "text", "up1", "str"),
        edge("e4", "tr_rise", "value", "m1", "in0"),
        edge("e5", "tr_rise", "matched", "m1", "in1"),
    ];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();

    let custom_outputs = HashMap::new();
    let decoder_states = HashMap::new();
    let mut fs_a = HashMap::new();
    let mut fs_b = HashMap::new();
    let mut ts_a: HashMap<String, TriggerState> = HashMap::new();
    let mut ts_b: HashMap<String, TriggerState> = HashMap::new();

    let compiled = g.compiled();
    let mut slots = vec![0.0f32; compiled.slot_count()];
    let mut written = vec![false; compiled.slot_count()];
    let mut str_slots = vec![String::new(); compiled.str_slot_count()];
    let mut str_written = vec![false; compiled.str_slot_count()];

    for frame_idx in 0..100u32 {
        // ch0: 5 帧高 (5.0) / 5 帧低 (0.0) 交替 — rising 每组触发一次, level 持续
        // ch1: 20 + (idx % 3) ∈ {20,21,22} — level 每帧命中 string 规则
        let ch0 = if frame_idx % 10 < 5 { 5.0 } else { 0.0 };
        let ch1 = 20.0 + (frame_idx % 3) as f32;
        let frames = source_frames(&[("proto1", vec![ch0, ch1])]);
        // 老路径: evaluate_into
        let mut out_a = ValuesMap::default();
        let mut out_str_a = StringValuesMap::default();
        g.evaluate_into(
            &frames,
            &empty_texts(),
            &HashMap::new(),
            &custom_outputs,
            &mut fs_a,
            &decoder_states,
            &mut HashMap::new(),
            &mut ts_a,
            &mut out_a,
            &mut out_str_a,
        );
        // 新路径: compiled.run + materialize / materialize_str (每帧清零)
        slots.fill(0.0);
        written.fill(false);
        str_slots.iter_mut().for_each(String::clear);
        str_written.fill(false);
        compiled.run(
            &frames,
            &empty_texts(),
            &HashMap::new(),
            &custom_outputs,
            &mut fs_b,
            &decoder_states,
            &mut HashMap::new(),
            &mut ts_b,
            &mut slots,
            &mut written,
            &mut str_slots,
            &mut str_written,
        );
        let mut out_b = ValuesMap::default();
        compiled.materialize(&slots, &written, &mut out_b);
        let mut out_str_b = StringValuesMap::default();
        compiled.materialize_str(&str_slots, &str_written, &mut out_str_b);
        assert_eq!(out_a, out_b, "帧 {frame_idx} 数值输出不一致");
        assert_eq!(out_str_a, out_str_b, "帧 {frame_idx} 字符串输出不一致");
    }

    // 收尾行为抽查 (两路径已逐帧等价, 这里验证 Trigger 语义确实被覆盖):
    // 帧 99: frame_idx % 10 = 9 → ch0 = 0 (tr_rise 未激活, 无输出); tr_lvl 命中
    assert!(ts_a.contains_key("tr_rise") && ts_a.contains_key("tr_lvl"));
    let mut out_a = ValuesMap::default();
    let mut out_str_a = StringValuesMap::default();
    let frames = source_frames(&[("proto1", vec![5.0, 20.0])]); // ch0 0→5 上升沿
    g.evaluate_into(
        &frames,
        &empty_texts(),
        &HashMap::new(),
        &custom_outputs,
        &mut fs_a,
        &decoder_states,
        &mut HashMap::new(),
        &mut ts_a,
        &mut out_a,
        &mut out_str_a,
    );
    // prev(帧99)=0 → 本帧 5.0 是上升沿: value=7, matched=1, m1 = 7+1 = 8
    assert_eq!(
        out_a.get("tr_rise").and_then(|m| m.get("value")),
        Some(&7.0)
    );
    assert_eq!(out_a.get("m1").and_then(|m| m.get("result")), Some(&8.0));
    // tr_lvl level 命中 string 规则 → text 经 Upper → "HI"; tr_man manual 命中
    assert_eq!(
        out_str_a.get("up1").and_then(|m| m.get("result")),
        Some(&"HI".to_string())
    );
    assert_eq!(
        out_str_a.get("tr_man").and_then(|m| m.get("text")),
        Some(&"ok".to_string())
    );
}
