//! 节点种类定义 (NodeKind) + 端口域 (PortDomain) 模型
//!
//! 图分为两个平面:
//! - **字节平面** (全局): Transport / Protocol / FrameDecoder 字节入口 /
//!   widget 的 loopbackOut 字节出口, 边携带 `Vec<u8>`, 事件驱动
//! - **数值平面** (每 tab 一张图): f32 槽位模型, ProtocolSource 引用全局
//!   Protocol 节点的最新帧 (source_frames)
//!
//! serde 约定: `NodeKind` 为 `#[serde(tag = "kind", content = "params")]`,
//! 前端 TS 镜像见 src/lib/utils/nodeDef.ts。

use serde::{Deserialize, Serialize};
use vofa_next_core::config::{ProtocolConfig, TransportConfig};
use vofa_next_dsp::{FilterKind, SpectrumOutput, WindowType};

use crate::decoder_block::DecoderBlockDef;
use crate::math_op::MathOp;

// ============ 端口 handle 命名约定 ============

/// Transport 节点的字节输出口 (RX 字节流出口)
pub const TRANSPORT_RX_HANDLE: &str = "rx";
/// Transport 节点的字节输入口 (TX 字节流入口)
pub const TRANSPORT_TX_HANDLE: &str = "tx";
/// Protocol 节点的字节输入口
pub const PROTOCOL_IN_HANDLE: &str = "in";
/// Protocol 节点的字节输出口 (解析后帧字节 / 透传字节出口)
pub const PROTOCOL_OUT_HANDLE: &str = "out";
/// FrameDecoder 节点的字节输入口 (新语义: 字节来源完全由输入字节边决定)
pub const FRAME_DECODER_IN_HANDLE: &str = "in";
/// FrameDecoder 旧版回环字节输入口 (保留兼容旧图数据)
pub const LOOPBACK_IN_HANDLE: &str = "loopbackIn";
/// widget 节点 (CommandSender 等) 的命令字节出口
pub const LOOPBACK_OUT_HANDLE: &str = "loopbackOut";

/// 节点种类 — 决定节点如何被评估
///
/// 注意: 不派生 PartialEq (TransportConfig/ProtocolConfig 未实现 PartialEq)。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "params")]
pub enum NodeKind {
    /// 传输层节点 (字节平面, 全局)
    /// 输出端口 "rx" (Bytes), 输入端口 "tx" (Bytes)
    Transport { config: TransportConfig },
    /// 协议引擎节点 (字节平面, 全局)
    /// 输入端口 "in" (Bytes), 输出端口 "out" (Bytes)
    /// convert_to: 可选的协议转换目标配置
    Protocol {
        config: ProtocolConfig,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        convert_to: Option<ProtocolConfig>,
    },
    /// 协议帧源 (数值平面) — 引用某个全局 Protocol 节点的最新帧
    /// 输出端口 "ch0".."chN" (F32), 求值时从 source_frames[node_id] 读取
    ProtocolSource {
        /// 被引用的全局 Protocol 节点 id
        node_id: String,
        /// 通道数 (输出 ch0..ch{channels-1})
        channels: usize,
    },
    /// 输入控件 (Knob/Slider/Button/Radio/Checkbox)
    /// 输出端口固定 "value", 值来自前端 invoke('set_input_value')
    Input,
    /// 算术节点
    /// 输出端口 "result"
    Math { op: MathOp, input_count: usize },
    /// 自定义 JS 节点
    /// 输入端口由用户代码定义, 输出端口由前端 iframe 回传
    /// 后端使用 custom_outputs 中的值作为节点输出
    Custom {
        /// 输入端口 id 列表 (前端解析代码后告诉后端)
        inputs: Vec<String>,
        /// 输出端口 id 列表
        outputs: Vec<String>,
    },
    /// 数字滤波器节点 (逐点运算, 融入 eval_order)
    /// 输入端口 "in0", 输出端口 "result"
    /// 后端维护滤波器状态 (FIR 延迟线 / IIR biquad 状态), 跨帧持久化
    /// 状态存储在 evaluate 的 filter_states 参数中, 由调用方管理生命周期
    Filter {
        /// 滤波器配置 (FIR coeffs 或 IIR biquad)
        kind: FilterKind,
    },
    /// 频谱分析节点 (块运算, 不在 eval_order)
    /// 输入端口 "in0", 无输出端口
    /// 后端维护滑动窗口, 由独立 30 FPS ticker 触发 FFT, 结果存入 spectrum_snapshot
    /// 通过 collect_spectrum_inputs 在每帧后从 output_snapshot 取输入值推入分析器
    SpectrumSink {
        /// FFT 窗口大小 (建议 2 的幂, 如 256/512/1024/2048)
        window_size: usize,
        /// 窗函数类型
        window_type: WindowType,
        /// 频谱输出模式
        output: SpectrumOutput,
        /// 采样率 (Hz), 用于计算频率轴
        sample_rate: f32,
    },
    /// 逆 FFT 节点 (频域→时域, 块运算, 融入 eval_order 输出时域流)
    /// 输入端口 "spectrum" (频域), 输出端口 "out0" (时域)
    /// 编译期从输入边解析出上游 FFT (SpectrumSink) 节点 id,
    /// 后端 spectrum_ticker 据此读取该 FFT 的频谱并合成时域缓冲,
    /// 本节点逐帧环形播放输出 (见 CompiledOp::Ifft)。
    Ifft,
    /// 帧解码节点 (SOURCE 类型, 输出来自字节流解析)
    ///
    /// 设计动机: 类似 CommandSender 但反向 — 字节流 → 按块定义解析 → 输出端口。
    /// 每个 field/bitfield 块对应一个输出端口, 另有可选 valid/frame_count/last_timestamp/fps 端口。
    ///
    /// 字节来源: 完全由输入字节边决定 (输入口 "in", 旧名 "loopbackIn" 兼容)。
    ///
    /// 跨帧状态: FrameParser 状态机由调用方 (data_loop) 管理,
    /// 字节流通过 feed_frame_decoders 推入, 解析完成后输出缓存到 decoder_states,
    /// evaluate 时从缓存读取最近一次解析结果。
    FrameDecoder {
        /// 块列表 (按顺序定义帧布局)
        blocks: Vec<DecoderBlockDef>,
        /// 附加输出端口开关 (与前端 FrameDecoderConfig 对应)
        enable_valid: bool,
        enable_frame_count: bool,
        enable_last_timestamp: bool,
        enable_fps: bool,
        /// Deprecated: 旧版回环模式标志。新语义下字节来源完全由输入字节边决定,
        /// 此字段不再影响编译/求值, 仅为旧数据反序列化兼容保留。
        #[serde(default)]
        loopback: bool,
    },
    /// Sink 节点 (Label/Gauge/LED/NumberDisplay/PieChart/Image/Waveform/Command)
    /// 这些节点没有 f32 输出, 后端 DAG 不评估它们, 前端通过 edges 自行查值;
    /// Command (CommandSender) 另有 "loopbackOut" 字节出口 (命令字节 → 字节平面)
    Sink,
}

/// 节点定义 — 通过 IPC 从前端同步到后端
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeDef {
    pub id: String,
    pub tab_id: String,
    pub kind: NodeKind,
}

// ============ 端口域 (PortDomain) ============

/// 端口域 — 边两端端口域必须一致, 否则编译报 DomainMismatch
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortDomain {
    /// 数值平面 (f32 槽位模型)
    F32,
    /// 字节平面 (Vec<u8>, 事件驱动)
    Bytes,
}

/// 查询节点某个端口的域
///
/// 端口域表:
/// - Transport: 输出 "rx" = Bytes, 输入 "tx" = Bytes
/// - Protocol: 输入 "in" = Bytes, 输出 "out" = Bytes
///   (chN 帧通道不经 Protocol 本体暴露, 数值平面用 ProtocolSource)
/// - FrameDecoder: 输入 "in" 与旧名 "loopbackIn" = Bytes, 其余输出 = F32
/// - Sink/Custom: 输出 "loopbackOut" = Bytes (CommandSender 命令字节出口)
/// - ProtocolSource: 输出 "ch0..chN" = F32; 其余节点按现有语义全 F32
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
        _ => PortDomain::F32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_port_domain_table() {
        let transport = NodeKind::Transport {
            config: TransportConfig::TestData(Default::default()),
        };
        assert_eq!(
            port_domain(&transport, TRANSPORT_RX_HANDLE, true),
            PortDomain::Bytes
        );
        assert_eq!(
            port_domain(&transport, TRANSPORT_TX_HANDLE, false),
            PortDomain::Bytes
        );

        let protocol = NodeKind::Protocol {
            config: ProtocolConfig::default(),
            convert_to: None,
        };
        assert_eq!(
            port_domain(&protocol, PROTOCOL_IN_HANDLE, false),
            PortDomain::Bytes
        );
        assert_eq!(
            port_domain(&protocol, PROTOCOL_OUT_HANDLE, true),
            PortDomain::Bytes
        );

        let decoder = NodeKind::FrameDecoder {
            blocks: vec![],
            enable_valid: false,
            enable_frame_count: false,
            enable_last_timestamp: false,
            enable_fps: false,
            loopback: false,
        };
        assert_eq!(
            port_domain(&decoder, FRAME_DECODER_IN_HANDLE, false),
            PortDomain::Bytes
        );
        assert_eq!(
            port_domain(&decoder, LOOPBACK_IN_HANDLE, false),
            PortDomain::Bytes
        );
        assert_eq!(port_domain(&decoder, "value", true), PortDomain::F32);

        let sink = NodeKind::Sink;
        assert_eq!(
            port_domain(&sink, LOOPBACK_OUT_HANDLE, true),
            PortDomain::Bytes
        );
        assert_eq!(port_domain(&sink, "value", false), PortDomain::F32);

        let source = NodeKind::ProtocolSource {
            node_id: "p1".into(),
            channels: 2,
        };
        assert_eq!(port_domain(&source, "ch0", true), PortDomain::F32);

        let math = NodeKind::Math {
            op: MathOp::Add,
            input_count: 2,
        };
        assert_eq!(port_domain(&math, "in0", false), PortDomain::F32);
        assert_eq!(port_domain(&math, "result", true), PortDomain::F32);
    }

    #[test]
    fn test_frame_decoder_loopback_default() {
        // 旧数据无 loopback 字段 → serde default 兼容
        let json = r#"{"kind":"FrameDecoder","params":{"blocks":[],"enable_valid":false,"enable_frame_count":false,"enable_last_timestamp":false,"enable_fps":false}}"#;
        let kind: NodeKind = serde_json::from_str(json).expect("旧数据应反序列化成功");
        assert!(matches!(kind, NodeKind::FrameDecoder { .. }));
    }
}
