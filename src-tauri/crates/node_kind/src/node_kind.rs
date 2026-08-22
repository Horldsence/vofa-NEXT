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

use dsp_fft::SpectrumOutput;
use dsp_filter::FilterKind;
use dsp_window::WindowType;
use schema_types::{DecoderBlockDef, ProtocolConfig, ProtocolSchema};
use serde::{Deserialize, Serialize};
use vofa_core::config::TransportConfig;

use node_trigger::TriggerRuleDef;

use crate::math_op::MathOp;
use crate::str_op::{StrNumParams, StrOp};

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
/// RawData 控件动态输入端口 id 前缀 (`src:<sourceId>:<sourceHandle>`)
/// 约定来源: 前端 rawDataPortId() (src/lib/utils/nodeDef.ts) — RawData 每个已连接的
/// (source, sourceHandle) 组合派生一个通道端口; 边只是用户意图标记, 字节不流入 f32 图
pub const RAW_DATA_PORT_PREFIX: &str = "src:";

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
    /// schema: 可选的帧 schema (协议引擎统一为 schema 模型; None = 旧前端, 按 config 构造引擎)
    Protocol {
        config: ProtocolConfig,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        convert_to: Option<ProtocolConfig>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        schema: Option<ProtocolSchema>,
    },
    /// 协议帧源 (数值平面) — 引用某个全局 Protocol 节点的最新帧
    /// 输出端口默认 "ch0".."chN" (F32), 求值时从 source_frames[node_id] 读取
    /// port_names: 可选命名端口 (schema 模型的端口名; None/空 = 缺省 ch0..chN)
    ProtocolSource {
        /// 被引用的全局 Protocol 节点 id
        node_id: String,
        /// 通道数 (输出 ch0..ch{channels-1} 或 port_names 各端口)
        channels: usize,
        /// 命名端口列表 (第 i 个名字对应 channels[i]; 缺省/越界回退 "ch{i}")
        #[serde(default, skip_serializing_if = "Option::is_none")]
        port_names: Option<Vec<String>>,
    },
    /// 输入控件 (Knob/Slider/Button/Radio/Checkbox)
    /// 输出端口固定 "value", 值来自前端 invoke('set_input_value')
    Input,
    /// 算术节点
    /// 输出端口 "result"
    Math { op: MathOp, input_count: usize },
    /// 字符串操作节点
    /// 输出端口固定 "result" (域由 op 决定: Len/Find/Contains 为 F32, 其余为 String)
    /// 输入端口见 StrOp::input_ports (str/str1/str2/substr 为 String, pos/len/size 为 F32)
    /// num: 未连接数值端口 (pos/len/size) 的内联回退值, 由前端内联框编辑后同步;
    ///      端口已连接时忽略, 求值使用上游值
    Str { op: StrOp, num: StrNumParams },
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
    /// 触发器节点 (Trigger)
    /// 由后端图求值驱动 (evaluate / CompiledEval): manual 模式每帧以 command 匹配,
    /// auto 模式对 "trigger" 输入端口上游值做边沿检测 (level/rising) 后匹配,
    /// 匹配状态跨帧持久于 trigger_states (regex/glob 缓存 + prev 值)。
    /// 输出端口:
    ///   - `value` (F32)   — number 规则的 output_value (string 规则命中时不覆盖)
    ///   - `matched` (F32) — 是否命中 (1/0)
    ///   - `text` (String) — string 规则的 output_text (number 命中/miss 时不覆盖)
    Trigger {
        /// 模式: 'manual' | 'auto'
        mode: String,
        /// 边沿: 'level' | 'rising' (仅 auto 模式生效)
        edge: String,
        /// 全部未命中时 value 端口的默认值
        default_miss: f32,
        /// 全部未命中时 text 端口的默认值
        default_miss_text: String,
        /// 当前待匹配命令字符串
        command: String,
        /// 规则列表
        rules: Vec<TriggerRuleDef>,
    },
    /// 文本输入节点 (TextInput) — 字符串输入源
    /// 前端文本框内容作为参数 text 经 update_tab_graph 同步;
    /// 求值时每帧原样写入字符串平面, 供下游 Str/TextDisplay 等消费。
    /// 输出端口固定 "str" (String), 无输入端口
    TextInput { text: String },
}

/// 解析 ProtocolSource 的输出端口名列表 (编译/求值共用):
/// port_names 给定且非空时用命名端口 (越界/空名回退 "ch{i}"), 否则缺省 "ch0..chN"
pub fn protocol_source_port_names(port_names: Option<&[String]>, channels: usize) -> Vec<String> {
    (0..channels)
        .map(|i| {
            port_names
                .and_then(|ps| ps.get(i))
                .filter(|p| !p.is_empty())
                .cloned()
                .unwrap_or_else(|| format!("ch{i}"))
        })
        .collect()
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
    /// 字符串平面 (String, 事件驱动; 与 graphOutputs 平行存在)
    String,
}

/// 查询节点某个端口的域
///
/// 端口域表:
/// - Transport: 输出 "rx" = Bytes, 输入 "tx" = Bytes
/// - Protocol: 输入 "in" = Bytes, 输出 "out" = Bytes
///   (chN 帧通道不经 Protocol 本体暴露, 数值平面用 ProtocolSource)
/// - FrameDecoder: 输入 "in" 与旧名 "loopbackIn" = Bytes, 其余输出 = F32
/// - Sink/Custom: 输出 "loopbackOut" = Bytes (CommandSender 命令字节出口)
/// - ProtocolSource: 输出 "ch0..chN" 或 port_names 命名端口 = F32; 其余节点按现有语义全 F32
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
        NodeKind::Str { op, .. } => {
            if is_output {
                // 输出端口统一命名 "result", 域由 op 决定; 未知端口回退 F32
                if handle == "result" {
                    op.output_domain()
                } else {
                    PortDomain::F32
                }
            } else {
                // 输入端口委托给 StrOp 端口表 (单一事实源); 未知端口回退 F32
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

    #[test]
    fn test_port_domain_table() {
        let transport = NodeKind::Transport {
            config: TransportConfig::TestData(vofa_core::config::TestDataConfig::default()),
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
            schema: None,
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
            port_names: None,
        };
        assert_eq!(port_domain(&source, "ch0", true), PortDomain::F32);

        let math = NodeKind::Math {
            op: MathOp::Add,
            input_count: 2,
        };
        assert_eq!(port_domain(&math, "in0", false), PortDomain::F32);
        assert_eq!(port_domain(&math, "result", true), PortDomain::F32);

        let str_len = NodeKind::Str {
            op: StrOp::Len,
            num: StrNumParams::default(),
        };
        assert_eq!(port_domain(&str_len, "str", false), PortDomain::String);
        assert_eq!(port_domain(&str_len, "result", true), PortDomain::F32);

        let str_mid = NodeKind::Str {
            op: StrOp::Mid,
            num: StrNumParams::default(),
        };
        assert_eq!(port_domain(&str_mid, "str", false), PortDomain::String);
        assert_eq!(port_domain(&str_mid, "pos", false), PortDomain::F32);
        assert_eq!(port_domain(&str_mid, "len", false), PortDomain::F32);
        assert_eq!(port_domain(&str_mid, "result", true), PortDomain::String);

        let str_replace = NodeKind::Str {
            op: StrOp::Replace,
            num: StrNumParams::default(),
        };
        assert_eq!(port_domain(&str_replace, "str1", false), PortDomain::String);
        assert_eq!(port_domain(&str_replace, "str2", false), PortDomain::String);
        assert_eq!(port_domain(&str_replace, "pos", false), PortDomain::F32);
        assert_eq!(
            port_domain(&str_replace, "result", true),
            PortDomain::String
        );

        let text_input = NodeKind::TextInput {
            text: "hello".to_string(),
        };
        assert_eq!(port_domain(&text_input, "str", true), PortDomain::String);
        // 未知端口/输入侧回退 F32
        assert_eq!(port_domain(&text_input, "str", false), PortDomain::F32);
        assert_eq!(port_domain(&text_input, "value", true), PortDomain::F32);
    }

    #[test]
    fn test_protocol_schema_and_port_names_default_compat() {
        // 旧前端: Protocol 无 schema 字段 / ProtocolSource 无 port_names 字段 → serde default 兼容
        let json = r#"{"kind":"Protocol","params":{"config":{"kind":"RawData"}}}"#;
        let kind: NodeKind = serde_json::from_str(json).expect("旧 Protocol 数据应反序列化成功");
        match kind {
            NodeKind::Protocol {
                schema, convert_to, ..
            } => {
                assert!(schema.is_none());
                assert!(convert_to.is_none());
            }
            other => panic!("expected Protocol, got {other:?}"),
        }

        let json = r#"{"kind":"ProtocolSource","params":{"node_id":"p1","channels":2}}"#;
        let kind: NodeKind =
            serde_json::from_str(json).expect("旧 ProtocolSource 数据应反序列化成功");
        match kind {
            NodeKind::ProtocolSource { port_names, .. } => assert!(port_names.is_none()),
            other => panic!("expected ProtocolSource, got {other:?}"),
        }
    }

    #[test]
    fn test_frame_decoder_loopback_default() {
        // 旧数据无 loopback 字段 → serde default 兼容
        let json = r#"{"kind":"FrameDecoder","params":{"blocks":[],"enable_valid":false,"enable_frame_count":false,"enable_last_timestamp":false,"enable_fps":false}}"#;
        let kind: NodeKind = serde_json::from_str(json).expect("旧数据应反序列化成功");
        assert!(matches!(kind, NodeKind::FrameDecoder { .. }));
    }

    #[test]
    fn test_text_input_serde_shape() {
        // serde 表示与前端 { kind: 'TextInput'; params: { text: string } } 对齐
        let json = r#"{"kind":"TextInput","params":{"text":"hi"}}"#;
        let kind: NodeKind = serde_json::from_str(json).expect("TextInput 应反序列化成功");
        match kind {
            NodeKind::TextInput { text } => assert_eq!(text, "hi"),
            other => panic!("expected TextInput, got {other:?}"),
        }
    }
}
