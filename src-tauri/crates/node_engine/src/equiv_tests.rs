//! 槽位等价性 / SpectrumSink / Ifft 求值测试

use std::collections::HashMap;

use node_kind::{DecoderBlockDef, MathOp, NodeDef, NodeKind};
use vofa_next_dsp::{FilterKind, IfftState, SpectrumOutput, WindowType};

use super::SourceFramesMap;
use crate::compile::CompiledGraph;
use crate::test_helpers::*;
use crate::ValuesMap;

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
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
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
        &HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashMap::new(),
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
            &HashMap::new(),
            &HashMap::new(),
            &mut HashMap::new(),
            &HashMap::new(),
            &mut ifft_states,
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
fn test_compiled_eval_equivalence() {
    let nodes = vec![
        make_protocol_source("ps1", "t1", "proto1", 4),
        make_math("m1", "t1", MathOp::Add, 2),
        make_math("m2", "t1", MathOp::Mul, 2),
        make_filter("f1", "t1", FilterKind::FIR { b: vec![0.5, 0.5] }),
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
        g.evaluate_into(
            &frames,
            &input_values,
            &custom_outputs,
            &mut fs_a,
            &decoder_states,
            &mut HashMap::new(),
            &mut out_a,
        );
        // 新路径: compiled.run + materialize (每帧清零 slots/written)
        slots.fill(0.0);
        written.fill(false);
        compiled.run(
            &frames,
            &input_values,
            &custom_outputs,
            &mut fs_b,
            &decoder_states,
            &mut HashMap::new(),
            &mut slots,
            &mut written,
        );
        let mut out_b = ValuesMap::default();
        compiled.materialize(&slots, &written, &mut out_b);
        assert_eq!(out_a, out_b, "帧 {} 输出不一致", frame_idx);
    }

    // FrameDecoder 无 parser: 两边都输出默认 0 端口 (value/valid/frame_count)
    let mut out_a = ValuesMap::default();
    let frames = source_frames(&[("proto1", vec![1.0, 2.0, 3.0, 4.0])]);
    g.evaluate_into(
        &frames,
        &input_values,
        &custom_outputs,
        &mut fs_a,
        &decoder_states,
        &mut HashMap::new(),
        &mut out_a,
    );
    let d1 = out_a.get("d1").expect("d1 应输出默认端口");
    assert_eq!(d1.get("value"), Some(&0.0));
    assert_eq!(d1.get("valid"), Some(&0.0));
    assert_eq!(d1.get("frame_count"), Some(&0.0));
    assert!(!d1.contains_key("last_timestamp")); // enable_last_timestamp = false
}
