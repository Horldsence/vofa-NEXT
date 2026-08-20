//! 图编译测试 — 拓扑序 / 循环检测 / 端口域分类 / 字节平面集成

use node_kind::{MathOp, NodeDef, NodeKind};
use vofa_next_dsp::{FilterKind, SpectrumOutput, WindowType};

use super::*;
use crate::test_helpers::*;

#[test]
fn test_compile_empty() {
    let g = CompiledGraph::compile("t1".into(), vec![], vec![]).unwrap();
    assert!(g.eval_order.is_empty());
    assert!(g.byte_plan().order.is_empty());
}

#[test]
fn test_cycle_detection() {
    let nodes = vec![
        make_math("a", "t1", MathOp::Add, 1),
        make_math("b", "t1", MathOp::Add, 1),
    ];
    let edges = vec![
        edge("e1", "a", "result", "b", "in0"),
        edge("e2", "b", "result", "a", "in0"),
    ];
    let result = CompiledGraph::compile("t1".into(), nodes, edges);
    assert!(matches!(result, Err(CompileError::Cycle)));
}

#[test]
fn test_sink_not_in_eval_order() {
    let nodes = vec![
        make_protocol_source("ps1", "t1", "proto1", 1),
        make_sink("gauge1", "t1"),
    ];
    let edges = vec![edge("e1", "ps1", "ch0", "gauge1", "value")];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    // Sink 不应在 eval_order 中
    assert!(!g.eval_order.contains(&"gauge1".to_string()));
    // ProtocolSource 应在 eval_order 中
    assert!(g.eval_order.contains(&"ps1".to_string()));
}

#[test]
fn test_filter_in_eval_order() {
    // Filter 应在 eval_order 中 (有输出)
    let nodes = vec![
        make_protocol_source("ps1", "t1", "proto1", 1),
        make_filter("f1", "t1", FilterKind::FIR { b: vec![1.0] }),
    ];
    let edges = vec![edge("e1", "ps1", "ch0", "f1", "in0")];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    assert!(g.eval_order.contains(&"f1".to_string()));
    assert!(g.filter_node_ids().contains(&"f1".to_string()));
}

#[test]
fn test_spectrum_sink_not_in_eval_order() {
    // SpectrumSink 不应在 eval_order 中 (无输出, 块运算)
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
    assert!(!g.eval_order.contains(&"s1".to_string()));
    assert!(g.eval_order.contains(&"ps1".to_string()));
    assert!(g.spectrum_sink_ids().contains(&"s1".to_string()));
}

#[test]
fn test_spectrum_sink_config() {
    let nodes = vec![
        make_protocol_source("ps1", "t1", "proto1", 1),
        make_spectrum_sink(
            "s1",
            "t1",
            512,
            WindowType::Blackman,
            SpectrumOutput::PSD,
            2000.0,
        ),
    ];
    let g = CompiledGraph::compile("t1".into(), nodes, vec![]).unwrap();
    let cfg = g.spectrum_sink_config("s1").expect("应能获取配置");
    assert_eq!(cfg.0, 512); // window_size
    assert_eq!(cfg.1, WindowType::Blackman); // window_type
    assert_eq!(cfg.2, SpectrumOutput::PSD); // output
    assert!((cfg.3 - 2000.0).abs() < 1e-6); // sample_rate

    // 不存在的节点应返回 None
    assert!(g.spectrum_sink_config("nonexistent").is_none());
}

#[test]
fn test_ifft_node_in_eval_order_and_source() {
    // Ifft 应在 eval_order 中 (有输出 out0), 且编译期解析出上游 FFT 源 id
    let nodes = vec![
        make_protocol_source("ps1", "t1", "proto1", 1),
        make_spectrum_sink(
            "fft1",
            "t1",
            256,
            WindowType::Hann,
            SpectrumOutput::Magnitude,
            1000.0,
        ),
        NodeDef {
            id: "ifft1".to_string(),
            tab_id: "t1".to_string(),
            kind: NodeKind::Ifft,
        },
    ];
    let edges = vec![
        edge("e1", "ps1", "ch0", "fft1", "in0"),
        edge("e2", "fft1", "spectrum", "ifft1", "spectrum"),
    ];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    assert!(g.eval_order.contains(&"ifft1".to_string()));
    assert!(g.ifft_node_ids().contains(&"ifft1".to_string()));
    assert_eq!(g.ifft_source("ifft1").as_deref(), Some("fft1"));
    // 无上游边时返回 None
    let g2 = CompiledGraph::compile(
        "t1".into(),
        vec![NodeDef {
            id: "ifft2".to_string(),
            tab_id: "t1".to_string(),
            kind: NodeKind::Ifft,
        }],
        vec![],
    )
    .unwrap();
    assert!(g2.ifft_source("ifft2").is_none());
}

// ============ 字节平面编译测试 ============

#[test]
fn test_transport_protocol_not_in_f32_eval_order() {
    // Transport/Protocol 是字节平面节点, 不进入 f32 eval_order
    let nodes = vec![
        make_transport("tp"),
        make_protocol("pt"),
        make_decoder("dec", "t1"),
    ];
    let edges = vec![
        edge("e1", "tp", "rx", "pt", "in"),
        edge("e2", "pt", "out", "dec", "in"),
    ];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    assert!(!g.eval_order.contains(&"tp".to_string()));
    assert!(!g.eval_order.contains(&"pt".to_string()));
    // FrameDecoder 有 f32 输出, 在 eval_order 中
    assert!(g.eval_order.contains(&"dec".to_string()));

    // 字节平面拓扑序: tp → pt → dec
    let plan = g.byte_plan();
    let pos = |id: &str| plan.order.iter().position(|n| n == id).unwrap();
    assert!(pos("tp") < pos("pt"));
    assert!(pos("pt") < pos("dec"));
    // 字节边单独分类
    assert_eq!(g.byte_edges().len(), 2);
}

#[test]
fn test_domain_mismatch_detected() {
    // Protocol.out (Bytes) → Math.in0 (F32): 域不匹配 → DomainMismatch
    let nodes = vec![make_protocol("pt"), make_math("m1", "t1", MathOp::Add, 1)];
    let edges = vec![edge("e1", "pt", "out", "m1", "in0")];
    let result = CompiledGraph::compile("t1".into(), nodes, edges);
    assert!(matches!(result, Err(CompileError::DomainMismatch(_))));

    // 反向: Math.result (F32) → Protocol.in (Bytes) 同样不匹配
    let nodes = vec![make_protocol("pt"), make_math("m1", "t1", MathOp::Add, 1)];
    let edges = vec![edge("e1", "m1", "result", "pt", "in")];
    let result = CompiledGraph::compile("t1".into(), nodes, edges);
    assert!(matches!(result, Err(CompileError::DomainMismatch(_))));
}

#[test]
fn test_cross_plane_loopback_no_false_cycle() {
    // 经典回环: FrameDecoder 输出 (F32) → Command 输入;
    // Command.loopbackOut (Bytes) → FrameDecoder.loopbackIn (Bytes)
    // 跨平面不构成循环: f32 平面只看 f32 边, 字节平面只看字节边 → 编译应成功
    let nodes = vec![make_decoder("dec", "t1"), make_sink("cmd", "t1")];
    let edges = vec![
        edge("e1", "dec", "value", "cmd", "value"),
        edge("e2", "cmd", "loopbackOut", "dec", "loopbackIn"),
    ];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    // 字节路由: cmd → dec (loopbackIn)
    let routes = g.byte_plan().routes_for("cmd");
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].target, "dec");
    assert_eq!(routes[0].target_handle, "loopbackIn");
}

#[test]
fn test_legacy_loopback_in_handle_is_bytes() {
    // 旧名 loopbackIn 与新名 in 均为 FrameDecoder 的字节入口
    let nodes = vec![make_transport("tp"), make_decoder("dec", "t1")];
    let edges = vec![edge("e1", "tp", "rx", "dec", "loopbackIn")];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    assert_eq!(g.byte_edges().len(), 1);
}

#[test]
fn test_same_id_protocol_and_protocol_source_coexist() {
    // 前端按 tab 提交的语义: 全局 Protocol 定义 (字节平面) 与本 tab 的
    // ProtocolSource 引用 (数值平面) 同 id。两者必须共存 —
    // ProtocolSource 的 ch 槽位不被 Protocol 覆盖 (否则通道恒读 0),
    // Protocol 仍在字节平面参与路由。
    let nodes = vec![
        make_protocol_source("pt", "t1", "pt", 2),
        make_protocol("pt"),
        make_transport("tp"),
        make_math("m1", "t1", MathOp::Add, 1),
    ];
    let edges = vec![
        edge("e1", "tp", "rx", "pt", "in"), // 字节边: Transport.rx → Protocol.in
        edge("e2", "pt", "ch0", "m1", "in0"), // 数值边: ProtocolSource.ch0 → Math
    ];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();

    // 数值平面: ProtocolSource 存活, ch 槽位已分配并参与求值
    assert!(g.eval_order.contains(&"pt".to_string()));
    assert!(g.compiled().slot_of("pt", "ch0").is_some());
    assert!(g.compiled().slot_of("pt", "ch1").is_some());
    let values = g.evaluate(
        &source_frames(&[("pt", vec![1.5, 2.5])]),
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &mut std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &mut std::collections::HashMap::new(),
    );
    assert_eq!(values.get("pt").and_then(|p| p.get("ch0")), Some(&1.5));
    assert_eq!(values.get("pt").and_then(|p| p.get("ch1")), Some(&2.5));
    assert_eq!(values.get("m1").and_then(|p| p.get("result")), Some(&1.5));

    // 字节平面: Protocol 定义生效, tp → pt 路由存在
    let routes = g.byte_plan().routes_for("tp");
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].target, "pt");
    assert_eq!(routes[0].target_handle, "in");
}

#[test]
fn test_raw_data_channel_edges_classified_by_source_domain() {
    // RawData 关联通道边 (Sink 的 src:<source>:<handle> 动态端口) 按源端域归类放行:
    // 字节源 (Transport.rx / Protocol.out) → byte_edges; 数值源 (chN) → f32 边 (不进 byte_edges)
    let nodes = vec![
        make_transport("tp"),
        make_protocol("pt"),
        make_protocol_source("pt", "t1", "pt", 1),
        make_sink("w-raw", "t1"),
    ];
    let edges = vec![
        edge("e-rx", "tp", "rx", "w-raw", "src:tp:rx"), // 种子场景: Transport.rx → RawData
        edge("e-out", "pt", "out", "w-raw", "src:pt:out"), // 种子场景: Protocol.out → RawData
        edge("e-ch", "pt", "ch0", "w-raw", "src:pt:ch0"), // 数值通道 (现状)
    ];
    let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
    let byte_ids: Vec<_> = g.byte_edges().iter().map(|e| e.id.as_str()).collect();
    assert_eq!(byte_ids, ["e-rx", "e-out"]);
}

#[test]
fn test_raw_data_prefix_on_non_sink_target_still_rejected() {
    // src: 前缀仅对 Sink (RawData) 放行 — 其他节点的跨域校验不受影响
    let nodes = vec![make_protocol("pt"), make_math("m1", "t1", MathOp::Add, 1)];
    let edges = vec![edge("e1", "pt", "out", "m1", "src:pt:out")];
    let result = CompiledGraph::compile("t1".into(), nodes, edges);
    assert!(matches!(result, Err(CompileError::DomainMismatch(_))));
}
