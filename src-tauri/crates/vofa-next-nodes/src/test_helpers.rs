//! 测试共享辅助 — 各模块测试共用的节点/边/帧源构造器

use vofa_next_buffer::graph::Edge;
use vofa_next_core::config::{ProtocolConfig, TransportConfig};
use vofa_next_core::DataFrame;
use vofa_next_dsp::{SpectrumOutput, WindowType};

use crate::eval::SourceFramesMap;
use crate::node_kind::{NodeDef, NodeKind};
use crate::{FilterKind, MathOp};

pub(crate) fn make_protocol_source(
    id: &str,
    tab_id: &str,
    node_id: &str,
    channels: usize,
) -> NodeDef {
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
pub(crate) fn make_protocol_source_named(
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
            port_names: Some(port_names.iter().map(|s| s.to_string()).collect()),
        },
    }
}

pub(crate) fn make_transport(id: &str) -> NodeDef {
    NodeDef {
        id: id.to_string(),
        tab_id: "t1".to_string(),
        kind: NodeKind::Transport {
            config: TransportConfig::TestData(Default::default()),
        },
    }
}

pub(crate) fn make_protocol(id: &str) -> NodeDef {
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

pub(crate) fn make_decoder(id: &str, tab_id: &str) -> NodeDef {
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

pub(crate) fn make_math(id: &str, tab_id: &str, op: MathOp, input_count: usize) -> NodeDef {
    NodeDef {
        id: id.to_string(),
        tab_id: tab_id.to_string(),
        kind: NodeKind::Math { op, input_count },
    }
}

pub(crate) fn make_input(id: &str, tab_id: &str) -> NodeDef {
    NodeDef {
        id: id.to_string(),
        tab_id: tab_id.to_string(),
        kind: NodeKind::Input,
    }
}

pub(crate) fn make_sink(id: &str, tab_id: &str) -> NodeDef {
    NodeDef {
        id: id.to_string(),
        tab_id: tab_id.to_string(),
        kind: NodeKind::Sink,
    }
}

pub(crate) fn make_custom(
    id: &str,
    tab_id: &str,
    inputs: Vec<&str>,
    outputs: Vec<&str>,
) -> NodeDef {
    NodeDef {
        id: id.to_string(),
        tab_id: tab_id.to_string(),
        kind: NodeKind::Custom {
            inputs: inputs.iter().map(|s| s.to_string()).collect(),
            outputs: outputs.iter().map(|s| s.to_string()).collect(),
        },
    }
}

pub(crate) fn make_filter(id: &str, tab_id: &str, kind: FilterKind) -> NodeDef {
    NodeDef {
        id: id.to_string(),
        tab_id: tab_id.to_string(),
        kind: NodeKind::Filter { kind },
    }
}

pub(crate) fn make_spectrum_sink(
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

pub(crate) fn edge(id: &str, src: &str, src_h: &str, tgt: &str, tgt_h: &str) -> Edge {
    Edge {
        id: id.to_string(),
        source: src.to_string(),
        source_handle: src_h.to_string(),
        target: tgt.to_string(),
        target_handle: tgt_h.to_string(),
    }
}

/// 构造多源最新帧缓存 (key = Protocol 节点 id)
pub(crate) fn source_frames(frames: &[(&str, Vec<f32>)]) -> SourceFramesMap {
    let mut m = SourceFramesMap::default();
    for (id, channels) in frames {
        m.insert(id.to_string(), DataFrame::new(channels.clone()));
    }
    m
}
