//! Filter 节点求值测试
#![allow(unused_imports, dead_code)]

use dsp_filter::{DigitalFilter, FilterConfig};
use node_kind::{MathOp, StrNumParams, StrOp};
use node_trigger::TriggerMatchType;

use super::*;
use crate::compile::CompiledGraph;
use crate::test_helpers::*;
#[test]
fn test_filter_lowpass_passes_input() {
    // 配置 Filter 的 FilterConfig 后, 单次评估应建立 filter_states 并产出非空结果
    let nodes = vec![
        make_protocol_source("ps1", "t1", "proto1", 1),
        make_filter("f1", "t1", FilterConfig::Lowpass { cutoff: 100.0, sample_rate: 1000.0 }),
    ];
    let edges = vec![edge("e1", "ps1", "ch0", "f1", "in0")];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let frames = source_frames(&[("proto1", vec![7.5])]);
    let mut filter_states = HashMap::new();
    let out = g.evaluate(
        &frames,
        &empty_texts(),
        &HashMap::new(),
        &HashMap::new(),
        &mut filter_states,
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut StringValuesMap::default(),
    );
    // Biquad 在 DC 处增益 < 1 (因 Butterworth 归一化到峰值), 输出应在 [0, 7.5] 内
    let v = out.get("f1").and_then(|m| m.get("result")).copied();
    assert!(v.is_some(), "filter result present");
    let r = v.unwrap();
    assert!(r.is_finite(), "filter result finite: {r}");
    assert!(r.abs() <= 7.5 + 1e-3, "filter not amplified past input: {r}");
    // filter_states 应包含 f1
    assert!(filter_states.contains_key("f1"));
}

#[test]
fn test_filter_state_persistence() {
    // filter_states 跨帧持久化 — 第二帧起输出基于非零状态 (与首帧不同)
    let nodes = vec![
        make_protocol_source("ps1", "t1", "proto1", 1),
        make_filter("f1", "t1", FilterConfig::Lowpass { cutoff: 100.0, sample_rate: 1000.0 }),
    ];
    let edges = vec![edge("e1", "ps1", "ch0", "f1", "in0")];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let mut filter_states = HashMap::new();

    let eval_with = |x: f32, fs: &mut HashMap<String, DigitalFilter>| -> f32 {
        let frames = source_frames(&[("proto1", vec![x])]);
        let out = g.evaluate(
            &frames,
            &empty_texts(),
            &HashMap::new(),
            &HashMap::new(),
            fs,
            &HashMap::new(),
            &mut HashMap::new(),
            &mut HashMap::new(),
            &mut StringValuesMap::default(),
        );
        out.get("f1")
            .and_then(|m| m.get("result"))
            .copied()
            .unwrap_or(0.0)
    };

    let _ = eval_with(1.0, &mut filter_states);
    let _ = eval_with(1.0, &mut filter_states);
    let _ = eval_with(1.0, &mut filter_states);
    // 持续 DC 输入 1.0, 低通稳态应趋向输入 (增益近似 1); 但有瞬态过程
    // 验证状态已建立: 第三帧结果应在 [0, 2] 区间内
    let r = eval_with(1.0, &mut filter_states);
    assert!(r.is_finite());
    assert!(r.abs() <= 2.0, "settled within reasonable range: {r}");
}

#[test]
fn test_filter_config_change_rebuilds_state() {
    // 用户修改 FilterConfig 时, 状态应重建 (按新派生 FilterKind 重新构造 DigitalFilter)
    let nodes = vec![
        make_protocol_source("ps1", "t1", "proto1", 1),
        make_filter("f1", "t1", FilterConfig::Lowpass { cutoff: 100.0, sample_rate: 1000.0 }),
    ];
    let edges = vec![edge("e1", "ps1", "ch0", "f1", "in0")];
    let g = CompiledGraph::compile("t1".into(), nodes, edges.clone()).unwrap();
    let mut filter_states = HashMap::new();

    // 帧 1
    let frames = source_frames(&[("proto1", vec![5.0])]);
    let _ = g.evaluate(
        &frames,
        &empty_texts(),
        &HashMap::new(),
        &HashMap::new(),
        &mut filter_states,
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut StringValuesMap::default(),
    );
    assert!(filter_states.contains_key("f1"));

    // FilterConfig 切换到 Bandpass, 与上一组 (Lowpass) 派生不同 FilterKind → 重建 state
    let nodes2 = vec![
        make_protocol_source("ps1", "t1", "proto1", 1),
        make_filter("f1", "t1", FilterConfig::Bandpass { low: 100.0, high: 200.0, sample_rate: 1000.0 }),
    ];
    let g2 = CompiledGraph::compile("t1".into(), nodes2, edges.clone()).unwrap();
    let frames2 = source_frames(&[("proto1", vec![3.0])]);
    let out2 = g2.evaluate(
        &frames2,
        &empty_texts(),
        &HashMap::new(),
        &HashMap::new(),
        &mut filter_states,
        &HashMap::new(),
        &mut HashMap::new(),
        &mut HashMap::new(),
        &mut StringValuesMap::default(),
    );
    // 重建后第一帧 — 输出为新 biquad 在零状态下的输出 (与旧 Lowpass 不同, 是有限值)
    let v = out2.get("f1").and_then(|m| m.get("result")).copied();
    assert!(v.is_some(), "filter result present after rebuild");
    let r = v.unwrap();
    assert!(r.is_finite(), "rebuilt filter output finite: {r}");
}

#[test]
fn test_filter_lowpass_preserves_dc() {
    // 低通滤波器对直流信号 (常数) 应基本保持原值
    let nodes = vec![
        make_protocol_source("ps1", "t1", "proto1", 1),
        make_filter(
            "f1",
            "t1",
            FilterConfig::Lowpass { cutoff: 100.0, sample_rate: 1000.0 },
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
            &empty_texts(),
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



