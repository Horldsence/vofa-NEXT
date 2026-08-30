//! # core
//!
//! VOFA-NEXT 基础类型 crate — 跨所有下游 crate 的共同类型基础。
//!
//! 模块:
// - [`frame`]: 数据帧 `DataFrame`、原始字节 `RawData`、连接状态、端口信息、传输统计。
// - [`serial_params`]: 串口基础参数 `Parity` / `StopBits` / `FlowControl`。
// - [`config`]: 传输层 (`TransportConfig` + 7 种 backend)、控件 (`WidgetConfig` + 9 种控件)
//!   与流水线 (`PipelineConfig`) 三组可调配置。
//!
//! 错误类型由独立 `error` crate 提供,本 crate 通过 `pub use` 兼容
//! `vofa_core::Error` / `vofa_core::Result` 调用路径。
//!
//! ## 设计原则
//!
//! 1. **单职责**:本 crate 仅承载跨域基础类型,**不依赖**任何 `protocol`/`buffer`/`nodes`/`automotive` 等。
//!    仅依赖 [`can_types`] 用于 `SlcanConfig`/`CandleConfig` 的 `CanBitrate`,以及
//!    [`error`] crate 提供的统一错误抽象。
//! 2. **serde 优先**:几乎所有类型派生 `Serialize`/`Deserialize`,便于与前端 IPC。
//! 3. **零业务**:仅数据载体,不包含协议解析/缓冲管理/调度逻辑。

pub mod config;
pub mod frame;
pub mod serial_params;

pub use config::{
    ButtonConfig, CandleConfig, CheckboxConfig, ImageConfig, ImageFormat, KnobConfig, LabelConfig,
    PieChartConfig, PipelineConfig, PipelineMode, RadioConfig, SerialConfig, SlcanConfig,
    SliderConfig, TcpClientConfig, TcpServerConfig, TestDataConfig, TestSignal, TransportConfig,
    UdpConfig, WaveformConfig, WidgetBinding, WidgetConfig,
};
pub use error::{AppError as Error, Result};
pub use frame::{now_us, ConnectionState, DataFrame, PortInfo, RawData, TransportStats};
pub use serial_params::{FlowControl, Parity, StopBits};
