//! # vofa-next-nodes
//!
//! 节点图 DAG 引擎 — 后端计算所有节点的输出值。
//!
//! 图分为两个平面:
//! - **字节平面** (全局): Transport / Protocol 节点、FrameDecoder 字节入口、
//!   widget 的 loopbackOut 字节出口; 边携带 Vec<u8>, 事件驱动 (见 [`BytePlan`])
//! - **数值平面** (每 tab 一张图, f32 槽位模型): ProtocolSource 引用全局
//!   Protocol 节点的最新帧 (source_frames 多源 latest-value 融合缓存)
//!
//! 核心类型:
//! - [`NodeKind`]: 节点种类 (Transport/Protocol/ProtocolSource/Input/Math/Custom/
//!   Filter/SpectrumSink/Ifft/FrameDecoder/Sink)
//! - [`NodeDef`]: 节点定义 (含 id/tab_id/kind)
//! - [`CompiledGraph`]: 编译后的 DAG, 含 f32 拓扑序 + 字节平面 BytePlan
//!
//! 数据流 (数值平面):
//!   source_frames → CompiledGraph.evaluate(source_frames, input_values, custom_outputs, ...)
//!            → HashMap<widgetId, HashMap<portId, f32>>  (所有节点的输出)
//!
//! 节点输出约定 (数值平面):
//! - ProtocolSource: 输出端口 "ch0", "ch1", ... (引用源的最新帧通道值)
//! - Input: 输出端口 "value" (来自前端 invoke)
//! - Math: 输出端口 "result"
//! - Custom: 输出端口由前端回传 (custom_outputs)
//! - Filter: 输出端口 "result" (逐点滤波, 融入 eval_order)
//! - SpectrumSink: 无输出 (块运算, 独立 30 FPS ticker 触发 FFT, 不在 eval_order)
//! - FrameDecoder: 输出端口来自 blocks 中的 field/bitfield + 可选 valid/frame_count/last_timestamp/fps
//!   (字节来源完全由输入字节边决定; 解析结果缓存在 decoder_states 中)
//! - Sink: 无 f32 输出 (纯消费; CommandSender 另有 loopbackOut 字节出口)
//!
//! 前端通过 edges 自行解析 Sink 的输入: 上游 widgetId + sourceHandle → 输出快照查值

pub mod byte_plan;
pub mod compile;
pub mod decoder_block;
pub mod eval;
pub mod evaluate;
pub mod frame_decoder;
pub mod math_op;
pub mod node_kind;

pub use byte_plan::{BytePlan, ByteRoute};
pub use compile::{CompileError, CompiledGraph};
pub use decoder_block::{
    DecoderBlockDef, DecoderChecksumCover, DecoderChecksumPosition, FieldType, LengthUnit,
};
pub use eval::{CompiledEval, SourceFramesMap};
pub use frame_decoder::{ChecksumAlgorithm, FrameParser, ParsedFrame};
pub use math_op::MathOp;
pub use node_kind::{
    port_domain, NodeDef, NodeKind, PortDomain, FRAME_DECODER_IN_HANDLE, LOOPBACK_IN_HANDLE,
    LOOPBACK_OUT_HANDLE, PROTOCOL_IN_HANDLE, PROTOCOL_OUT_HANDLE, TRANSPORT_RX_HANDLE,
    TRANSPORT_TX_HANDLE,
};
pub use vofa_next_dsp::{
    DigitalFilter, FilterKind, FilterPreset, IfftState, SpectrumOutput, WindowType,
};

use rustc_hash::FxBuildHasher;
use std::collections::HashMap;

/// 图输出值表 (热路径) — FxHash 替代 SipHash, 高码率逐帧覆盖写时查找快 3~5 倍。
/// serde 对任意 BuildHasher+Default 的 HashMap 透明, 线上 JSON 格式不变。
pub type ValuesMap = HashMap<String, HashMap<String, f32, FxBuildHasher>, FxBuildHasher>;

#[cfg(test)]
pub(crate) mod test_helpers;
