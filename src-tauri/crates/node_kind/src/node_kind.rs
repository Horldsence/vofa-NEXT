//! 节点种类（`NodeKind`）— 决定节点如何被评估.
//!
//! 端口域模型与端口查询在 `crate::ports`，`NodeDef` 在 `crate::spec`.
//!
//! serde 约定：`#[serde(tag = "kind", content = "params")]`，前端 TS 镜像见
//! `src/lib/utils/nodeDef.ts`。

use dsp_fft::SpectrumOutput;
use dsp_filter::FilterConfig;
use dsp_window::WindowType;
use schema_types::{DecoderBlockDef, ProtocolConfig, ProtocolSchema};
use serde::{Deserialize, Serialize};
use vofa_core::config::TransportConfig;

use node_trigger::TriggerRuleDef;

use crate::math_op::MathOp;
use crate::ports::RAW_DATA_PORT_PREFIX;
use crate::str_op::{StrNumParams, StrOp};

// ============ 节点种类枚举 ============

/// 节点种类 — 决定节点如何被评估
///
/// 注意：不派生 `PartialEq`（`TransportConfig`/`ProtocolConfig` 未实现 `PartialEq`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "params")]
pub enum NodeKind {
    /// 传输层节点（字节平面，全局）
    /// 输出端口 "rx"（Bytes），输入端口 "tx"（Bytes）
    Transport { config: TransportConfig },
    /// 协议引擎节点（字节平面，全局）
    /// 输入端口 "in"（Bytes），输出端口 "out"（Bytes）
    /// `convert_to`：可选的协议转换目标配置
    /// `schema`：可选的帧 schema（协议引擎统一为 schema 模型；`None` = 旧前端，按 config 构造引擎）
    Protocol {
        config: ProtocolConfig,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        convert_to: Option<ProtocolConfig>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        schema: Option<ProtocolSchema>,
    },
    /// 协议帧源（数值平面）— 引用某个全局 Protocol 节点的最新帧
    /// 输出端口默认 `"ch0".."chN"`（F32），求值时从 `source_frames[node_id]` 读取
    /// `port_names`：可选命名端口（schema 模型的端口名；None/空 = 缺省 ch0..chN）
    ProtocolSource {
        node_id: String,
        channels: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        port_names: Option<Vec<String>>,
    },
    /// 输入控件（Knob/Slider/Button/Radio/Checkbox）
    /// 输出端口固定 "value"，值来自前端 `invoke('set_input_value')`
    Input,
    /// 算术节点 — 输出端口 "result"
    Math { op: MathOp, input_count: usize },
    /// 字符串操作节点 — 输出端口固定 "result"（域由 op 决定）
    /// 输入端口见 `StrOp::input_ports`，`num`：未连接数值端口的内联回退值
    Str { op: StrOp, num: StrNumParams },
    /// 自定义 JS 节点 — 输入/输出端口由用户代码定义
    Custom {
        inputs: Vec<String>,
        outputs: Vec<String>,
    },
    /// 数字滤波器节点（逐点运算，融入 `eval_order`）
    /// 输入端口 "in0"，输出端口 "result"
    Filter { config: FilterConfig },
    /// 频谱分析节点（块运算，不在 `eval_order`）
    /// 输入端口 "in0"，无输出端口（结果走 `spectrum_snapshot` 订阅旁路）
    SpectrumSink {
        window_size: usize,
        window_type: WindowType,
        output: SpectrumOutput,
        sample_rate: f32,
    },
    /// 逆 FFT 节点（频域→时域，块运算，融入 `eval_order` 输出时域流）
    /// 输入端口 "spectrum"，输出端口 "out0"
    Ifft,
    /// 帧解码节点（SOURCE 类型，输出来自字节流解析）
    /// 每个 field/bitfield 块对应一个输出端口，另有可选 valid/frame_count/last_timestamp/fps 端口
    FrameDecoder {
        blocks: Vec<DecoderBlockDef>,
        enable_valid: bool,
        enable_frame_count: bool,
        enable_last_timestamp: bool,
        enable_fps: bool,
        /// Deprecated：旧版回环模式标志。新语义下字节来源完全由输入字节边决定，
        /// 此字段不再影响编译/求值，仅为旧数据反序列化兼容保留。
        #[serde(default)]
        loopback: bool,
    },
    /// Sink 节点（Label/Gauge/LED/NumberDisplay/PieChart/Image/Waveform/Command）
    /// 无 f32 输出；Command 另有 "loopbackOut" 字节出口
    Sink,
    /// 触发器节点（Trigger）
    /// `manual` 模式每帧以 `command` 匹配；`auto` 模式对 "trigger" 输入边沿检测（level/rising）
    /// 匹配状态跨帧持久于 `trigger_states`
    Trigger {
        mode: String,
        edge: String,
        default_miss: f32,
        default_miss_text: String,
        command: String,
        rules: Vec<TriggerRuleDef>,
    },
    /// 文本输入节点（TextInput）— 字符串输入源
    /// 前端文本框内容作为参数 `text` 经 `update_tab_graph` 同步
    /// 输出端口固定 "str"（String），无输入端口
    TextInput { text: String },
}

/// 协议帧源节点是否参与字节源标记边（Sink 视角）
pub const fn is_protocol_source(k: &NodeKind) -> bool {
    matches!(k, NodeKind::ProtocolSource { .. })
}

/// RawData 控件的端口是否采用 `src:` 动态端口前缀约定
pub fn is_raw_data_handle(handle: &str) -> bool {
    handle.starts_with(RAW_DATA_PORT_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::PortDomain;

    #[test]
    fn protocol_schema_and_port_names_default_compat() {
        // 旧前端：Protocol 无 schema 字段 / ProtocolSource 无 port_names 字段 → serde default 兼容
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
    fn frame_decoder_loopback_default() {
        let json = r#"{"kind":"FrameDecoder","params":{"blocks":[],"enable_valid":false,"enable_frame_count":false,"enable_last_timestamp":false,"enable_fps":false}}"#;
        let kind: NodeKind = serde_json::from_str(json).expect("旧数据应反序列化成功");
        assert!(matches!(kind, NodeKind::FrameDecoder { .. }));
    }

    #[test]
    fn text_input_serde_shape() {
        let json = r#"{"kind":"TextInput","params":{"text":"hi"}}"#;
        let kind: NodeKind = serde_json::from_str(json).expect("TextInput 应反序列化成功");
        match kind {
            NodeKind::TextInput { text } => assert_eq!(text, "hi"),
            other => panic!("expected TextInput, got {other:?}"),
        }
    }

    #[test]
    fn raw_data_handle_prefix() {
        assert!(is_raw_data_handle("src:tp:rx"));
        assert!(!is_raw_data_handle("src"));
        assert!(!is_raw_data_handle("loopbackOut"));
    }

    #[test]
    fn protocol_source_predicate() {
        assert!(is_protocol_source(&NodeKind::ProtocolSource {
            node_id: "p".into(),
            channels: 1,
            port_names: None,
        }));
        assert!(!is_protocol_source(&NodeKind::Input));
        assert!(!is_protocol_source(&NodeKind::Sink));
    }

    #[test]
    fn port_domain_import_path_unaffected() {
        // 端口域仍通过 `node_kind::PortDomain` 暴露，pub use 路径不破坏
        let _: PortDomain = PortDomain::F32;
    }
}
