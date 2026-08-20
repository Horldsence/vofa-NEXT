//! # vofa-next-core (façade)
//!
//! **Stage A 收尾**:本 crate 已转为纯 re-export 壳,所有实际类型迁出至下游 5 个 crate:
//!
//! | 子 crate           | 承载类型                                              |
//! |--------------------|------------------------------------------------------|
//! | [`vofa_core`]      | Error/Result, DataFrame/RawData, ConnectionState/PortInfo/TransportStats,<br>Parity/StopBits/FlowControl, TransportConfig + 7 backend, WidgetConfig + 9 widget, PipelineConfig |
//! | [`can_types`]      | CanFrame/CanBuffer/CanBitrate/CanLoadStats/CandleDeviceInfo ... |
//! | [`logic_types`]    | LogicSample/LogicBuffer/DecodedEvent/LogicDecoderConfig ... |
//! | [`diagnostic`]     | UdsService/ObdMode/J1939Id/DiagnosticConfig ...       |
//! | [`schema_types`]   | ChecksumAlgorithm/DecoderBlockDef/ProtocolSchema/ProtocolConfig/parse_hex/encode_by_blocks ... |
//!
//! 所有原有 `vofa_next_core::TypeName` 路径在根命名空间下仍然可用;
//! 旧的 `vofa_next_core::config::*` / `::can::*` / `::logic::*` / `::diagnostic::*`
//! 子模块路径**已删除**,调用方请改用根路径或直接用底层 crate。
//!
//! ## Stage H (清理阶段)
//!
//! 本 crate 最终将被整体移除,所有调用方改用 5 个底层 crate。façade 仅作短期过渡。

pub use can_types::*;
pub use diagnostic::*;
pub use logic_types::*;
pub use schema_types::*;
pub use vofa_core::config::*;
pub use vofa_core::serial_params::*;
pub use vofa_core::{
    now_us, DataFrame, Error, PortInfo, RawData, Result, TransportStats, ConnectionState,
};