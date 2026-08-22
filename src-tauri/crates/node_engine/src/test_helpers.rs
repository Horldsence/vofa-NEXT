//! 测试共享辅助 — 各模块测试共用的节点/边/帧源构造器

use buffer_graph::Edge;
use dsp_fft::SpectrumOutput;
use dsp_filter::FilterKind;
use dsp_window::WindowType;
use schema_types::ProtocolConfig;
use vofa_core::config::TransportConfig;
use vofa_core::DataFrame;

use node_kind::{MathOp, NodeDef, NodeKind, StrNumParams, StrOp};

use crate::SourceFramesMap;

pub fn make_protocol_source(id: &str, tab_id: &str, node_id: &str, channels: usize) -> NodeDef {
    NodeDef {
        id: id.to_string(),
        tab_id: tab_id.to_string(),
        kind: NodeKind::ProtocolSource {
            node_id: node_id.to_string(),
            channels,
            port_names: None,
        },
    }
}

/// 带命名端口的 ProtocolSource (schema 模型; port_names 与 channels 对齐)
pub fn make_protocol_source_named(
    id: &str,
    tab_id: &str,
    node_id: &str,
    port_names: &[&str],
) -> NodeDef {
    NodeDef {
        id: id.to_string(),
        tab_id: tab_id.to_string(),
        kind: NodeKind::ProtocolSource {
            node_id: node_id.to_string(),
            channels: port_names.len(),
            port_names: Some(port_names.iter().map(ToString::to_string).collect()),
        },
    }
}

pub fn make_transport(id: &str) -> NodeDef {
    NodeDef {
        id: id.to_string(),
        tab_id: "t1".to_string(),
        kind: NodeKind::Transport {
            config: TransportConfig::TestData(vofa_core::config::TestDataConfig::default()),
        },
    }
}

pub fn make_protocol(id: &str) -> NodeDef {
    NodeDef {
        id: id.to_string(),
        tab_id: "t1".to_string(),
        kind: NodeKind::Protocol {
            config: ProtocolConfig::default(),
            convert_to: None,
            schema: None,
        },
    }
}

pub fn make_decoder(id: &str, tab_id: &str) -> NodeDef {
    NodeDef {
        id: id.to_string(),
        tab_id: tab_id.to_string(),
        kind: NodeKind::FrameDecoder {
            blocks: vec![],
            enable_valid: false,
            enable_frame_count: false,
            enable_last_timestamp: false,
            enable_fps: false,
            loopback: false,
        },
    }
}

pub fn make_math(id: &str, tab_id: &str, op: MathOp, input_count: usize) -> NodeDef {
    NodeDef {
        id: id.to_string(),
        tab_id: tab_id.to_string(),
        kind: NodeKind::Math { op, input_count },
    }
}

pub fn make_input(id: &str, tab_id: &str) -> NodeDef {
    NodeDef {
        id: id.to_string(),
        tab_id: tab_id.to_string(),
        kind: NodeKind::Input,
    }
}

pub fn make_sink(id: &str, tab_id: &str) -> NodeDef {
    NodeDef {
        id: id.to_string(),
        tab_id: tab_id.to_string(),
        kind: NodeKind::Sink,
    }
}

pub fn make_custom(id: &str, tab_id: &str, inputs: Vec<&str>, outputs: Vec<&str>) -> NodeDef {
    NodeDef {
        id: id.to_string(),
        tab_id: tab_id.to_string(),
        kind: NodeKind::Custom {
            inputs: inputs.iter().map(ToString::to_string).collect(),
            outputs: outputs.iter().map(ToString::to_string).collect(),
        },
    }
}

pub fn make_filter(id: &str, tab_id: &str, kind: FilterKind) -> NodeDef {
    NodeDef {
        id: id.to_string(),
        tab_id: tab_id.to_string(),
        kind: NodeKind::Filter { kind },
    }
}

/// Str 节点 (默认内联数值参数: pos=1, len=0, size=0)
pub fn make_str(id: &str, tab_id: &str, op: StrOp) -> NodeDef {
    make_str_num(id, tab_id, op, StrNumParams::default())
}

/// Str 节点 (显式内联数值参数)
pub fn make_str_num(id: &str, tab_id: &str, op: StrOp, num: StrNumParams) -> NodeDef {
    NodeDef {
        id: id.to_string(),
        tab_id: tab_id.to_string(),
        kind: NodeKind::Str { op, num },
    }
}

pub fn make_spectrum_sink(
    id: &str,
    tab_id: &str,
    window_size: usize,
    window_type: WindowType,
    output: SpectrumOutput,
    sample_rate: f32,
) -> NodeDef {
    NodeDef {
        id: id.to_string(),
        tab_id: tab_id.to_string(),
        kind: NodeKind::SpectrumSink {
            window_size,
            window_type,
            output,
            sample_rate,
        },
    }
}

pub fn edge(id: &str, src: &str, src_h: &str, tgt: &str, tgt_h: &str) -> Edge {
    Edge {
        id: id.to_string(),
        source: src.to_string(),
        source_handle: src_h.to_string(),
        target: tgt.to_string(),
        target_handle: tgt_h.to_string(),
    }
}

/// 构造多源最新帧缓存 (key = Protocol 节点 id)
pub fn source_frames(frames: &[(&str, Vec<f32>)]) -> SourceFramesMap {
    let mut m = SourceFramesMap::default();
    for (id, channels) in frames {
        m.insert(id.to_string(), DataFrame::new(channels.clone()));
    }
    m
}
