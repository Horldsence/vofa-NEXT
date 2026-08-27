//! 端口 handle 常量 + 端口域（`PortDomain`）+ `port_domain` 查询函数.
//!
//! 端口命名与域分类在图编译期使用，是边分类（`node_engine::EdgeClass`）的依据。

use crate::node_kind::NodeKind;

// ============ 端口 handle 命名约定 ============

/// Transport 节点的字节输出口（RX 字节流出口）
pub const TRANSPORT_RX_HANDLE: &str = "rx";
/// Transport 节点的字节输入口（TX 字节流入口）
pub const TRANSPORT_TX_HANDLE: &str = "tx";
/// Protocol 节点的字节输入口
pub const PROTOCOL_IN_HANDLE: &str = "in";
/// Protocol 节点的字节输出口（解析后帧字节 / 透传字节出口）
pub const PROTOCOL_OUT_HANDLE: &str = "out";
/// FrameDecoder 节点的字节输入口（新语义：字节来源完全由输入字节边决定）
pub const FRAME_DECODER_IN_HANDLE: &str = "in";
/// FrameDecoder 旧版回环字节输入口（保留兼容旧图数据）
pub const LOOPBACK_IN_HANDLE: &str = "loopbackIn";
/// widget 节点（CommandSender 等）的命令字节出口
pub const LOOPBACK_OUT_HANDLE: &str = "loopbackOut";
/// RawData 控件动态输入端口 id 前缀（`src:<sourceId>:<sourceHandle>`）
///
/// 约定来源：前端 `rawDataPortId()`（`src/lib/utils/nodeDef.ts`）— RawData 每个已连接的
/// (source, sourceHandle) 组合派生一个通道端口；边只是用户意图标记，字节不流入 f32 图
pub const RAW_DATA_PORT_PREFIX: &str = "src:";

// ============ 端口域（PortDomain）============

/// 端口域 — 边两端端口域必须一致，否则编译报 DomainMismatch
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortDomain {
    /// 数值平面（f32 槽位模型）
    F32,
    /// 字节平面（`Vec<u8>`，事件驱动）
    Bytes,
    /// 字符串平面（`String`，事件驱动；与 graphOutputs 平行存在）
    String,
}

/// 查询节点某个端口的域
///
/// 端口域表：
/// - Transport: 输出 "rx" = Bytes，输入 "tx" = Bytes
/// - Protocol: 输入 "in" = Bytes，输出 "out" = Bytes
///   （chN 帧通道不经 Protocol 本体暴露，数值平面用 ProtocolSource）
/// - FrameDecoder: 输入 "in" 与旧名 "loopbackIn" = Bytes，其余输出 = F32
/// - Sink/Custom: 输出 "loopbackOut" = Bytes（CommandSender 命令字节出口）
/// - ProtocolSource: 输出 "str"（RawData 原始字节文本）= String；
///   其余输出（"ch0".."chN" 或 port_names 命名端口）= F32；其余节点按现有语义全 F32
pub fn port_domain(kind: &NodeKind, handle: &str, is_output: bool) -> PortDomain {
    match kind {
        NodeKind::Transport { .. } => match (is_output, handle) {
            (true, TRANSPORT_RX_HANDLE) | (false, TRANSPORT_TX_HANDLE) => PortDomain::Bytes,
            _ => PortDomain::F32,
        },
        NodeKind::Protocol { .. } => match (is_output, handle) {
            (true, PROTOCOL_OUT_HANDLE) | (false, PROTOCOL_IN_HANDLE) => PortDomain::Bytes,
            _ => PortDomain::F32,
        },
        NodeKind::FrameDecoder { .. }
            if !is_output
                && (handle == FRAME_DECODER_IN_HANDLE || handle == LOOPBACK_IN_HANDLE) =>
        {
            PortDomain::Bytes
        }
        NodeKind::Sink | NodeKind::Custom { .. } if is_output && handle == LOOPBACK_OUT_HANDLE => {
            PortDomain::Bytes
        }
        NodeKind::Trigger { .. } if is_output && handle == "text" => PortDomain::String,
        NodeKind::TextInput { .. } if is_output && handle == "str" => PortDomain::String,
        // ProtocolSource 的 "str" 端口（RawData 原始字节 UTF-8 文本）属字符串平面
        NodeKind::ProtocolSource { .. } if is_output && handle == "str" => PortDomain::String,
        NodeKind::Str { op, .. } => {
            if is_output {
                // 输出端口统一命名 "result"，域由 op 决定；未知端口回退 F32
                if handle == "result" {
                    op.output_domain()
                } else {
                    PortDomain::F32
                }
            } else {
                // 输入端口委托给 StrOp 端口表（单一事实源）；未知端口回退 F32
                op.input_ports()
                    .iter()
                    .find(|(name, _)| *name == handle)
                    .map_or(PortDomain::F32, |(_, domain)| *domain)
            }
        }
        _ => PortDomain::F32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vofa_core::config::TestDataConfig;

    use crate::math_op::MathOp;
    use crate::node_kind::NodeKind;
    use crate::str_op::{StrNumParams, StrOp};

    fn transport_test_data() -> NodeKind {
        NodeKind::Transport {
            config: vofa_core::config::TransportConfig::TestData(TestDataConfig::default()),
        }
    }

    fn protocol_default() -> NodeKind {
        NodeKind::Protocol {
            config: schema_types::ProtocolConfig::default(),
            convert_to: None,
            schema: None,
        }
    }

    fn decoder() -> NodeKind {
        NodeKind::FrameDecoder {
            blocks: vec![],
            enable_valid: false,
            enable_frame_count: false,
            enable_last_timestamp: false,
            enable_fps: false,
            loopback: false,
        }
    }

    fn sink() -> NodeKind {
        NodeKind::Sink
    }

    fn source() -> NodeKind {
        NodeKind::ProtocolSource {
            node_id: "p1".into(),
            channels: 2,
            port_names: None,
        }
    }

    fn math() -> NodeKind {
        NodeKind::Math {
            op: MathOp::Add,
            input_count: 2,
        }
    }

    fn str_op(op: StrOp) -> NodeKind {
        NodeKind::Str {
            op,
            num: StrNumParams::default(),
        }
    }

    fn text_input() -> NodeKind {
        NodeKind::TextInput {
            text: "hello".to_string(),
        }
    }

    #[test]
    fn transport_handles() {
        let k = transport_test_data();
        assert_eq!(
            port_domain(&k, TRANSPORT_RX_HANDLE, true),
            PortDomain::Bytes
        );
        assert_eq!(
            port_domain(&k, TRANSPORT_TX_HANDLE, false),
            PortDomain::Bytes
        );
    }

    #[test]
    fn protocol_handles() {
        let k = protocol_default();
        assert_eq!(
            port_domain(&k, PROTOCOL_IN_HANDLE, false),
            PortDomain::Bytes
        );
        assert_eq!(
            port_domain(&k, PROTOCOL_OUT_HANDLE, true),
            PortDomain::Bytes
        );
    }

    #[test]
    fn decoder_handles() {
        let k = decoder();
        assert_eq!(
            port_domain(&k, FRAME_DECODER_IN_HANDLE, false),
            PortDomain::Bytes
        );
        assert_eq!(
            port_domain(&k, LOOPBACK_IN_HANDLE, false),
            PortDomain::Bytes
        );
        assert_eq!(port_domain(&k, "value", true), PortDomain::F32);
    }

    #[test]
    fn sink_and_command_loopback_out() {
        let k = sink();
        assert_eq!(
            port_domain(&k, LOOPBACK_OUT_HANDLE, true),
            PortDomain::Bytes
        );
        assert_eq!(port_domain(&k, "value", false), PortDomain::F32);
    }

    #[test]
    fn protocol_source_str_handle_is_string_domain() {
        let k = source();
        assert_eq!(port_domain(&k, "ch0", true), PortDomain::F32);
        assert_eq!(port_domain(&k, "str", true), PortDomain::String);
        assert_eq!(port_domain(&k, "str", false), PortDomain::F32);
    }

    #[test]
    fn math_input_output_is_f32() {
        let k = math();
        assert_eq!(port_domain(&k, "in0", false), PortDomain::F32);
        assert_eq!(port_domain(&k, "result", true), PortDomain::F32);
    }

    #[test]
    fn str_op_port_table_is_source_of_truth() {
        let len = str_op(StrOp::Len);
        assert_eq!(port_domain(&len, "str", false), PortDomain::String);
        assert_eq!(port_domain(&len, "result", true), PortDomain::F32);

        let mid = str_op(StrOp::Mid);
        assert_eq!(port_domain(&mid, "str", false), PortDomain::String);
        assert_eq!(port_domain(&mid, "pos", false), PortDomain::F32);
        assert_eq!(port_domain(&mid, "len", false), PortDomain::F32);
        assert_eq!(port_domain(&mid, "result", true), PortDomain::String);

        let replace = str_op(StrOp::Replace);
        assert_eq!(port_domain(&replace, "str1", false), PortDomain::String);
        assert_eq!(port_domain(&replace, "str2", false), PortDomain::String);
        assert_eq!(port_domain(&replace, "pos", false), PortDomain::F32);
        assert_eq!(port_domain(&replace, "result", true), PortDomain::String);
    }

    #[test]
    fn text_input_str_handle_is_string_output_only() {
        let k = text_input();
        assert_eq!(port_domain(&k, "str", true), PortDomain::String);
        assert_eq!(port_domain(&k, "str", false), PortDomain::F32);
        assert_eq!(port_domain(&k, "value", true), PortDomain::F32);
    }
}
