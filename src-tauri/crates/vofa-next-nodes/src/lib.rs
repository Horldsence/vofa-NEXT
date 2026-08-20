//! # vofa-next-nodes (façade)
//!
//! 节点图 DAG 引擎 — 后端计算所有节点的输出值。
//!
//! Stage D.2 后,本 crate 已转为纯 `pub use` 聚合层,所有实现迁至 4 个领域 crate:
//!
//! - [`node_trigger`]: 触发器匹配器 (Exact/Prefix/Contains/Regex/Range/Glob)
//! - [`node_frame_decoder`]: 帧解码状态机 (字节流 → 帧解析 → 输出端口值)
//! - [`node_kind`]: 节点种类系统 (NodeKind/NodeDef/PortDomain/MathOp + DecoderBlockDef)
//! - [`node_engine`]: 节点图引擎 (BytePlan + Compile + Eval + Evaluate + ValuesMap)
//!
//! 图分为两个平面:
//! - **字节平面** (全局): Transport / Protocol / FrameDecoder 字节入口 /
//!   widget 的 loopbackOut 字节出口; 边携带 `Vec<u8>`, 事件驱动
//! - **数值平面** (每 tab 一张图, f32 槽位模型): ProtocolSource 引用全局
//!   Protocol 节点的最新帧 (source_frames 多源 latest-value 融合缓存)
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
//! - Sink: 无 f32 输出 (纯消费; CommandSender 另有 loopbackOut 字节出口)
//!
//! 前端通过 edges 自行解析 Sink 的输入: 上游 widgetId + sourceHandle → 输出快照查值

// ============ node_trigger ============
pub use node_trigger::{
    parse_range, TriggerMatchResult, TriggerMatchType, TriggerMatcher, TriggerRuleDef,
};

// ============ node_frame_decoder ============
pub use node_frame_decoder::{
    parse_hex, ChecksumAlgorithm, FrameDecoderTestData, FrameParser, ParsedFrame,
};

// ============ node_kind ============
pub use node_kind::{
    port_domain, protocol_source_port_names, AsciiBase, DecoderBlockDef, DecoderChecksumCover,
    DecoderChecksumPosition, FieldType, LengthUnit, MathOp, NodeDef, NodeKind, PortDomain,
    FRAME_DECODER_IN_HANDLE, LOOPBACK_IN_HANDLE, LOOPBACK_OUT_HANDLE, PROTOCOL_IN_HANDLE,
    PROTOCOL_OUT_HANDLE, RAW_DATA_PORT_PREFIX, TRANSPORT_RX_HANDLE, TRANSPORT_TX_HANDLE,
};

// ============ node_engine ============
pub use node_engine::{
    BytePlan, ByteRoute, CompiledEval, CompiledGraph, CompileError, CompiledOp, SourceFramesMap,
    ValuesMap,
};

// ============ DSP 重导出 (向后兼容 vofa_next_nodes::DigitalFilter 等) ============
pub use vofa_next_dsp::{
    DigitalFilter, FilterKind, FilterPreset, IfftState, SpectrumOutput, WindowType,
};