//! 图编译派生数据 — 节点输出端口表与生效通道数 (后端单一权威)
//!
//! 由 [`compute_derived`] 在 `apply_tab_graph` 编译成功后计算,
//! 随 `update_tab_graph` Tauri 命令返回并经 `graph:derived` 事件差分推送。
//! 前端 `derivedPorts` store 据此渲染 React Flow handle 与节点摘要。

use kind::{protocol_source_port_names, NodeDef, NodeKind, PortDomain};
use schema_types::{ProtocolConfig, ProtocolSchema};
use serde::Serialize;

/// 端口域 wire 序列化形态 — 与前端 `domain: 'F32' | 'Bytes' | 'String'` 对齐
///
/// `kind::PortDomain` 是无 serde 派生的轻量枚举;为 IPC 契约在 derived 模块内
/// 独立序列化,避免反向修改 kind 的公共 API。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum DerivedPortDomain {
    F32,
    Bytes,
    String,
    Spectrum,
}

impl From<PortDomain> for DerivedPortDomain {
    fn from(d: PortDomain) -> Self {
        match d {
            PortDomain::F32 => Self::F32,
            PortDomain::Bytes => Self::Bytes,
            PortDomain::String => Self::String,
            PortDomain::Spectrum => Self::Spectrum,
        }
    }
}

/// 单个输出端口的派生元数据 (与前端 `NodeDerived.ports[]` 对应)
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct NodeDerivedPort {
    /// 端口 id (handle 字符串)
    pub name: String,
    /// 端口承载域 (F32 / Bytes / String)
    pub domain: DerivedPortDomain,
}

/// 单节点派生数据
///
/// - `ports` 仅含**输出**端口;输入端口由消费方 (下游节点) 从自身 ports 表查
/// - `effective_channels` 仅 Protocol 节点有意义 (None = 其他节点)
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct NodeDerived {
    pub node_id: String,
    pub ports: Vec<NodeDerivedPort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_channels: Option<usize>,
}

/// 图级派生数据 — 多个节点的批 (随命令响应 / 事件推送)
///
/// `version` 为提交成功后的全局图版本号 (前端以此作为下次提交的 base_version,
/// 用于多写入方冲突检测)
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GraphDerived {
    pub nodes: Vec<NodeDerived>,
    pub version: u64,
}

/// 给定一组节点定义, 派生每个节点的输出端口表与生效通道数
///
/// - 全局 Protocol 节点:端口表由 `ProtocolSchema::port_names()` (custom) 或
///   `protocol_source_port_names(channels)` (preset, RawData → 单 `str` 字符串域) 决定;
///   `effective_channels` = 手动配置值或回退默认
/// - widget 节点 (Input/Math/Str/Custom/Filter/Fft/Ifft/FrameDecoder/Trigger/
///   TextInput/Sink/ProtocolSource): 由 [`kind_output_ports`] 按 [`PortDomain`] 表枚举
/// - Transport 节点:不参与数值平面图渲染,跳过 (字节平面节点由 `BytePlan` 路由)
pub fn compute_derived(nodes: &[NodeDef]) -> Vec<NodeDerived> {
    nodes
        .iter()
        .filter_map(|n| {
            if matches!(n.kind, NodeKind::Transport { .. }) {
                return None;
            }
            Some(derive_node(n))
        })
        .collect()
}

fn derive_node(n: &NodeDef) -> NodeDerived {
    match &n.kind {
        NodeKind::Protocol { config, schema, .. } => {
            derive_protocol(n.id.clone(), config, schema.as_ref())
        }
        NodeKind::ProtocolSource { .. } => NodeDerived {
            // 端口表与所引用的全局 Protocol 节点同源 — 渲染端按全局节点 id 读,
            // 此处重复一份便于初次派生到达前的占位查询
            node_id: n.id.clone(),
            ports: Vec::new(),
            effective_channels: None,
        },
        NodeKind::Sink => NodeDerived {
            // Sink 视 widget 类型有不同 ports (Waveform ch0..N, PieChart seg0..N, ...);
            // 当前 widget 配置不在 NodeDef 中, 由前端 widgetToNodeKind 已隐式处理 —
            // 此处返回空, 前端继续走本地 widgetPorts() (widget spec 派生, 非图编译推导)
            node_id: n.id.clone(),
            ports: Vec::new(),
            effective_channels: None,
        },
        _ => NodeDerived {
            node_id: n.id.clone(),
            ports: kind_output_ports(&n.kind),
            effective_channels: None,
        },
    }
}

fn derive_protocol(
    node_id: String,
    config: &ProtocolConfig,
    schema: Option<&ProtocolSchema>,
) -> NodeDerived {
    let (ports, channels) = match schema {
        // Custom: 端口由 decode 块派生, 通道数 = 端口数
        Some(s) if s.preset == schema_types::SchemaPreset::Custom => {
            let names = s.port_names();
            let ports = names
                .iter()
                .map(|p| NodeDerivedPort {
                    name: p.clone(),
                    domain: domain_for_protocol_port(p),
                })
                .collect();
            (ports, names.len())
        }
        // preset + 有 schema: 端口由 schema (legacyConfig) 决定; 无 channels 字段时用默认值
        Some(_) => {
            let names = preset_port_names(config);
            let ports = names
                .iter()
                .map(|p| NodeDerivedPort {
                    name: p.clone(),
                    domain: domain_for_protocol_port(p),
                })
                .collect();
            let n = names.len();
            (ports, n)
        }
        // preset + 无 schema (前端省略 schema 时):按 config 工厂构造, 端口同 preset
        None => {
            let names = preset_port_names(config);
            let ports = names
                .iter()
                .map(|p| NodeDerivedPort {
                    name: p.clone(),
                    domain: domain_for_protocol_port(p),
                })
                .collect();
            let n = names.len();
            (ports, n)
        }
    };
    NodeDerived {
        node_id,
        ports,
        effective_channels: Some(channels),
    }
}

/// preset 协议端口表 — 与 `protocol_source_port_names` 一致,
/// 额外处理 RawData (单 "str" 字符串域, 无 chN)
fn preset_port_names(config: &ProtocolConfig) -> Vec<String> {
    if matches!(config, ProtocolConfig::RawData) {
        return vec!["str".to_string()];
    }
    let n = protocol_channels_from_config(config);
    protocol_source_port_names(None, n)
}

/// Protocol 节点端口域: ch0..chN 数值域 (F32); "str" (RawData 原始字节文本) 字符串域
fn domain_for_protocol_port(name: &str) -> DerivedPortDomain {
    if name == "str" {
        DerivedPortDomain::String
    } else {
        DerivedPortDomain::F32
    }
}

/// 从 ProtocolConfig 提取默认通道数
fn protocol_channels_from_config(config: &ProtocolConfig) -> usize {
    match config {
        ProtocolConfig::JustFloat { channels } | ProtocolConfig::FireWater { channels } => {
            channels.unwrap_or(4)
        }
        // RawData/Slcan/CandleLight/LogicDecode/Diagnostic 不产数值帧,
        // 端口表为空 (compile  端口数 = 0), 仅 "str" 字符串域 (RawData)
        _ => 0,
    }
}

/// 按 NodeKind 枚举其标准输出端口 (与 [`kind::port_domain`] 镜像)
///
/// 端口域由 [`DerivedPortDomain`] 决定;与 [`port_domain`] 单一事实源同步 (手工镜像以
/// 避免双向依赖 `port_domain` 的 is_output 反向枚举)。
fn kind_output_ports(kind: &NodeKind) -> Vec<NodeDerivedPort> {
    match kind {
        NodeKind::Input => vec![port("value", DerivedPortDomain::F32)],
        NodeKind::Math { .. } => vec![port("result", DerivedPortDomain::F32)],
        NodeKind::Str { op, .. } => vec![port("result", op.output_domain().into())],
        NodeKind::TextInput { .. } => vec![port("str", DerivedPortDomain::String)],
        NodeKind::Fft { .. } => {
            vec![port("spectrum", DerivedPortDomain::Spectrum)]
        }
        NodeKind::Ifft => vec![port("out0", DerivedPortDomain::F32)],
        NodeKind::FrameDecoder { .. } => {
            // FrameDecoder 输出端口由 blocks 决定, 不在 NodeDef 中携带 (前端 widget spec)
            Vec::new()
        }
        NodeKind::Trigger { .. } => vec![
            port("value", DerivedPortDomain::F32),
            port("matched", DerivedPortDomain::F32),
            port("text", DerivedPortDomain::String),
        ],
        NodeKind::Custom { outputs, .. } => outputs
            .iter()
            .map(|n| port(n.as_str(), DerivedPortDomain::F32))
            .collect(),
        // Sink / Filter / Transport: 前端 widget spec 派生, 非图编译推导
        _ => Vec::new(),
    }
}

fn port(name: &str, domain: DerivedPortDomain) -> NodeDerivedPort {
    NodeDerivedPort {
        name: name.to_string(),
        domain,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schema_types::{DecoderBlockDef, FieldType, SchemaPreset};

    fn protocol_node(id: &str, config: ProtocolConfig, schema: Option<ProtocolSchema>) -> NodeDef {
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

    #[test]
    fn custom_schema_derives_named_ports() {
        let schema = ProtocolSchema {
            preset: SchemaPreset::Custom,
            legacy_config: None,
            decode: vec![
                DecoderBlockDef::Field {
                    id: "f1".into(),
                    port_name: "voltage".into(),
                    field_type: FieldType::Float32LE,
                    length_ref: None,
                    match_id: None,
                },
                DecoderBlockDef::Field {
                    id: "f2".into(),
                    port_name: "current".into(),
                    field_type: FieldType::Float32LE,
                    length_ref: None,
                    match_id: None,
                },
            ],
            encode: None,
        };
        let n = protocol_node(
            "p1",
            ProtocolConfig::JustFloat { channels: None },
            Some(schema),
        );
        let derived = compute_derived(&[n]);
        assert_eq!(derived.len(), 1);
        assert_eq!(
            derived[0].ports,
            vec![
                NodeDerivedPort {
                    name: "voltage".into(),
                    domain: DerivedPortDomain::F32
                },
                NodeDerivedPort {
                    name: "current".into(),
                    domain: DerivedPortDomain::F32
                },
            ]
        );
        assert_eq!(derived[0].effective_channels, Some(2));
    }

    #[test]
    fn preset_justfloat_with_manual_channels_uses_config_value() {
        let n = protocol_node("p1", ProtocolConfig::JustFloat { channels: Some(6) }, None);
        let derived = compute_derived(&[n]);
        let names: Vec<_> = derived[0]
            .ports
            .iter()
            .map(|p| (p.name.as_str(), p.domain))
            .collect();
        assert_eq!(
            names,
            vec![
                ("ch0", DerivedPortDomain::F32),
                ("ch1", DerivedPortDomain::F32),
                ("ch2", DerivedPortDomain::F32),
                ("ch3", DerivedPortDomain::F32),
                ("ch4", DerivedPortDomain::F32),
                ("ch5", DerivedPortDomain::F32),
            ]
        );
        assert_eq!(derived[0].effective_channels, Some(6));
    }

    #[test]
    fn preset_justfloat_auto_falls_back_to_default() {
        let n = protocol_node("p1", ProtocolConfig::JustFloat { channels: None }, None);
        let derived = compute_derived(&[n]);
        assert_eq!(derived[0].ports.len(), 4);
        assert_eq!(derived[0].effective_channels, Some(4));
    }

    #[test]
    fn rawdata_protocol_uses_str_string_domain_only() {
        let n = protocol_node("p1", ProtocolConfig::RawData, None);
        let derived = compute_derived(&[n]);
        assert_eq!(derived[0].ports.len(), 1);
        assert_eq!(derived[0].ports[0].name, "str");
        assert_eq!(derived[0].ports[0].domain, DerivedPortDomain::String);
        assert_eq!(derived[0].effective_channels, Some(1));
    }

    #[test]
    fn non_protocol_kinds_produce_known_ports() {
        let input = NodeDef {
            id: "i1".into(),
            tab_id: "tab1".into(),
            kind: NodeKind::Input,
        };
        let math = NodeDef {
            id: "m1".into(),
            tab_id: "tab1".into(),
            kind: NodeKind::Math {
                op: kind::MathOp::Add,
                input_count: 3,
            },
        };
        let text_in = NodeDef {
            id: "t1".into(),
            tab_id: "tab1".into(),
            kind: NodeKind::TextInput { text: "hi".into() },
        };
        let derived = compute_derived(&[input, math, text_in]);
        assert_eq!(
            derived[0].ports,
            vec![port("value", DerivedPortDomain::F32)]
        );
        assert_eq!(
            derived[1].ports,
            vec![port("result", DerivedPortDomain::F32)]
        );
        assert_eq!(
            derived[2].ports,
            vec![port("str", DerivedPortDomain::String)]
        );
    }

    #[test]
    fn transport_nodes_are_skipped() {
        let t = NodeDef {
            id: "t1".into(),
            tab_id: "tab1".into(),
            kind: NodeKind::Transport {
                config: vofa_core::config::TransportConfig::TestData(
                    vofa_core::config::TestDataConfig::default(),
                ),
            },
        };
        let derived = compute_derived(&[t]);
        assert!(derived.is_empty());
    }
}
