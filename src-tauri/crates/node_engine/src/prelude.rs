//! `node_engine` 公共调用面 — 调用方 `use node_engine::prelude::*;` 获得全套类型.
//!
//! 包含：
//! - 编译前端: `Edge`, `NodeDef`, `NodeKind`, `PortDomain`
//! - 编译产物: `CompiledGraph`, `TypedGraph`, `BytePlan`, `CompiledEval`, `CompiledOp`
//! - 求值工具: `SourceFramesMap`, `SourceTextsMap`, `TriggerState`, `FrameParser`
//! - 共用范式 trait: `NodeSpec`, `Compilable`, `Evaluable`

// 该模块是统一导入面 — re-export 项目随节点引擎扩展而增加, 允许未使用项
#![allow(unused_imports)]

// ============ 下游 crate 类型 ============
pub use buffer_graph::{Edge, NodeGraph, RoutedData};
pub use dsp_fft::{IfftState, SpectrumOutput, WindowType};
pub use dsp_filter::DigitalFilter;
pub use node_frame_decoder::FrameParser;
pub use node_kind::{
    DecoderBlockDef, MathOp, NodeDef, NodeKind, PortDomain, StrNumParams, StrOp, StrResult,
    FRAME_DECODER_IN_HANDLE, LOOPBACK_IN_HANDLE, LOOPBACK_OUT_HANDLE, PROTOCOL_IN_HANDLE,
    PROTOCOL_OUT_HANDLE, RAW_DATA_PORT_PREFIX, TRANSPORT_RX_HANDLE, TRANSPORT_TX_HANDLE,
};
pub use node_trigger::{TriggerRuleDef, TriggerState};

// ============ 引擎类型 (流水线各段 crate) + 本 crate 门面 ============
pub use crate::compile::CompiledGraph;
pub use crate::traits::{
    Compilable, CompileInput, CompileOutput, EvalInput, EvalOutput, Evaluable, NodeSpec,
    PortDescriptor, PortKind,
};
pub use node_eval::{CompiledEval, SourceFramesMap, SourceTextsMap};
pub use node_hir::{
    port_domain_event, CompileError, CompileReport, EdgeClass, HirEdge, HirNode, TypedGraph,
};
pub use node_lower::CompiledOp;
pub use node_plane::{BytePlan, ByteRoute, ValueMir};
