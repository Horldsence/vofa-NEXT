//! ProtocolSource NodeDef 自动注入 — 取代前端 `syncTabGraphToBackend` 内的手工注入
//!
//! 前端 `update_tab_graph` 只提交原始节点 (widget + 全局 Transport/Protocol) + 边;
//! 后端编译时按"本 tab 内存在从某全局 Protocol 节点输出端口出发的边"自动追加
//! 对应 ProtocolSource NodeDef, 供 `CompiledGraph::compile` 与 `evaluate` 消费。
//!
//! ProtocolSource 节点的 `id` = 被引用的全局 Protocol 节点 id (与原前端约定一致),
//! `port_names` 与 `channels` 由 `protocol_source_port_names` / `ProtocolSchema::port_names`
//! 推导 (后端单一权威)。

use buffer_graph::Edge;
use node_kind::{protocol_source_port_names, NodeDef, NodeKind};
use schema_types::ProtocolConfig;
use std::collections::HashSet;

/// 给定原始节点表 + 边, 返回需要追加到编译列表的 ProtocolSource NodeDef
///
/// 输入边为前端提交的本 tab 边 (含 widget ↔ widget / widget ↔ global);
/// ProtocolSource 仅在边**起点**为全局 Protocol 节点时追加 (id 与全局节点 id 重合)。
pub fn inject_protocol_sources(nodes: &[NodeDef], edges: &[Edge]) -> Vec<NodeDef> {
    // 收集全局 Protocol 节点 (候选源)
    let protocols: Vec<&NodeDef> = nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Protocol { .. }))
        .collect();
    if protocols.is_empty() {
        return Vec::new();
    }
    // 已注入去重: 同一全局 Protocol 节点只生成一份 ProtocolSource
    let mut emitted: HashSet<String> = HashSet::new();
    let mut sources: Vec<NodeDef> = Vec::new();
    for e in edges {
        // 边起点为全局 Protocol 节点 → 追加 ProtocolSource
        let proto = match protocols.iter().find(|p| p.id == e.source) {
            Some(p) => *p,
            None => continue,
        };
        if !emitted.insert(proto.id.clone()) {
            continue;
        }
        let (channels, port_names) = protocol_output_spec(proto);
        sources.push(NodeDef {
            id: proto.id.clone(),
            tab_id: proto.tab_id.clone(),
            kind: NodeKind::ProtocolSource {
                node_id: proto.id.clone(),
                channels,
                port_names: Some(port_names),
            },
        });
    }
    sources
}

/// 全局 Protocol 节点的输出端口 spec (channels + port_names)
///
/// - custom schema: 端口名 = `ProtocolSchema::port_names()`, 通道数 = 端口数
/// - preset: 端口名 = `protocol_source_port_names(None, channels)` (ch0..chN, RawData → 单 `str`)
fn protocol_output_spec(node: &NodeDef) -> (usize, Vec<String>) {
    let kind = &node.kind;
    let NodeKind::Protocol { config, schema, .. } = kind else {
        return (0, Vec::new());
    };
    match schema {
        Some(s) if s.preset == schema_types::SchemaPreset::Custom => {
            let names = s.port_names();
            (names.len(), names)
        }
        Some(_) | None => {
            // preset (有 schema 但非 Custom) 或 无 schema (前端已省略) — 均走 preset 路径
            if matches!(config, ProtocolConfig::RawData) {
                return (1, vec!["str".to_string()]);
            }
            let n = channels_from_config(config);
            let names = protocol_source_port_names(None, n);
            (n, names)
        }
    }
}

fn channels_from_config(config: &ProtocolConfig) -> usize {
    match config {
        ProtocolConfig::JustFloat { channels } | ProtocolConfig::FireWater { channels } => {
            channels.unwrap_or(4)
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use buffer_graph::Edge;
    use node_kind::NodeKind;
    use schema_types::{DecoderBlockDef, FieldType, SchemaPreset};
    use vofa_core::config::TransportConfig;

    fn protocol_node(
        id: &str,
        config: ProtocolConfig,
        schema: Option<schema_types::ProtocolSchema>,
    ) -> NodeDef {
        NodeDef {
            id: id.into(),
            tab_id: "tab1".into(),
            kind: NodeKind::Protocol {
                config,
                convert_to: None,
                schema,
            },
        }
    }

    fn edge(source: &str, source_handle: &str, target: &str, target_handle: &str) -> Edge {
        Edge {
            id: format!("e-{source}-{target}"),
            source: source.into(),
            source_handle: source_handle.into(),
            target: target.into(),
            target_handle: target_handle.into(),
        }
    }

    #[test]
    fn empty_edges_yields_no_injection() {
        let n = protocol_node("p1", ProtocolConfig::JustFloat { channels: Some(4) }, None);
        assert!(inject_protocol_sources(&[n], &[]).is_empty());
    }

    #[test]
    fn edge_from_protocol_emits_single_protocol_source() {
        let proto = protocol_node("p1", ProtocolConfig::JustFloat { channels: Some(4) }, None);
        let widget = NodeDef {
            id: "w1".into(),
            tab_id: "tab1".into(),
            kind: NodeKind::Sink,
        };
        let edges = vec![edge("p1", "ch0", "w1", "in0")];
        let sources = inject_protocol_sources(&[proto, widget], &edges);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].id, "p1");
        let NodeKind::ProtocolSource {
            node_id,
            channels,
            port_names,
        } = &sources[0].kind
        else {
            panic!("expected ProtocolSource");
        };
        assert_eq!(node_id, "p1");
        assert_eq!(*channels, 4);
        assert_eq!(
            port_names.as_ref().unwrap().as_slice(),
            &[
                "ch0".to_string(),
                "ch1".to_string(),
                "ch2".to_string(),
                "ch3".to_string()
            ]
        );
    }

    #[test]
    fn multiple_edges_to_same_protocol_emit_once() {
        let proto = protocol_node("p1", ProtocolConfig::JustFloat { channels: Some(2) }, None);
        let widget_a = NodeDef {
            id: "wa".into(),
            tab_id: "tab1".into(),
            kind: NodeKind::Sink,
        };
        let widget_b = NodeDef {
            id: "wb".into(),
            tab_id: "tab1".into(),
            kind: NodeKind::Sink,
        };
        let edges = vec![
            edge("p1", "ch0", "wa", "in0"),
            edge("p1", "ch1", "wb", "in0"),
        ];
        let sources = inject_protocol_sources(&[proto, widget_a, widget_b], &edges);
        assert_eq!(sources.len(), 1);
    }

    #[test]
    fn edge_from_transport_does_not_emit_protocol_source() {
        let transport = NodeDef {
            id: "t1".into(),
            tab_id: "tab1".into(),
            kind: NodeKind::Transport {
                config: TransportConfig::TestData(Default::default()),
            },
        };
        let widget = NodeDef {
            id: "w1".into(),
            tab_id: "tab1".into(),
            kind: NodeKind::Sink,
        };
        let edges = vec![edge("t1", "rx", "w1", "data")];
        let sources = inject_protocol_sources(&[transport, widget], &edges);
        assert!(sources.is_empty());
    }

    #[test]
    fn custom_schema_emits_named_ports() {
        let schema = schema_types::ProtocolSchema {
            preset: SchemaPreset::Custom,
            legacy_config: None,
            decode: vec![DecoderBlockDef::Field {
                id: "f1".into(),
                port_name: "rpm".into(),
                field_type: FieldType::Float32LE,
                length_ref: None,
                match_id: None,
            }],
            encode: None,
        };
        let proto = protocol_node(
            "p1",
            ProtocolConfig::JustFloat { channels: None },
            Some(schema),
        );
        let widget = NodeDef {
            id: "w1".into(),
            tab_id: "tab1".into(),
            kind: NodeKind::Sink,
        };
        let edges = vec![edge("p1", "rpm", "w1", "in0")];
        let sources = inject_protocol_sources(&[proto, widget], &edges);
        assert_eq!(sources.len(), 1);
        let NodeKind::ProtocolSource {
            channels,
            port_names,
            ..
        } = &sources[0].kind
        else {
            panic!();
        };
        assert_eq!(*channels, 1);
        assert_eq!(
            port_names.as_ref().unwrap().as_slice(),
            &["rpm".to_string()]
        );
    }

    #[test]
    fn rawdata_protocol_emits_str_only() {
        let proto = protocol_node("p1", ProtocolConfig::RawData, None);
        let widget = NodeDef {
            id: "w1".into(),
            tab_id: "tab1".into(),
            kind: NodeKind::Sink,
        };
        let edges = vec![edge("p1", "str", "w1", "in0")];
        let sources = inject_protocol_sources(&[proto, widget], &edges);
        assert_eq!(sources.len(), 1);
        let NodeKind::ProtocolSource {
            channels,
            port_names,
            ..
        } = &sources[0].kind
        else {
            panic!();
        };
        assert_eq!(*channels, 1);
        assert_eq!(
            port_names.as_ref().unwrap().as_slice(),
            &["str".to_string()]
        );
    }

    /// 与 derived 端 `preset_justfloat_auto_falls_back_to_default` 对齐:
    /// `channels: None` 时 inject 也回退到默认 4 端口, 避免两侧派生不一致
    #[test]
    fn preset_justfloat_auto_channels_uses_default_four() {
        let proto = protocol_node("p1", ProtocolConfig::JustFloat { channels: None }, None);
        let widget = NodeDef {
            id: "w1".into(),
            tab_id: "tab1".into(),
            kind: NodeKind::Sink,
        };
        let edges = vec![edge("p1", "ch0", "w1", "in0")];
        let sources = inject_protocol_sources(&[proto, widget], &edges);
        assert_eq!(sources.len(), 1);
        let NodeKind::ProtocolSource {
            channels,
            port_names,
            ..
        } = &sources[0].kind
        else {
            panic!("expected ProtocolSource");
        };
        assert_eq!(*channels, 4);
        assert_eq!(
            port_names.as_ref().unwrap().as_slice(),
            &[
                "ch0".to_string(),
                "ch1".to_string(),
                "ch2".to_string(),
                "ch3".to_string()
            ]
        );
    }
}
