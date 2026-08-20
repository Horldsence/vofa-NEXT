//! # core
//!
//! VOFA-NEXT 基础类型 crate — 跨所有下游 crate 的共同类型基础。
//!
//! 模块:
//! - [`error`]: 统一错误类型 `Error` + `Result<T>` 别名,实现 `serde::Serialize` 用于 IPC。
//! - [`frame`]: 数据帧 `DataFrame`、原始字节 `RawData`、连接状态、端口信息、传输统计。
//!
//! 注意:`config` 模块暂留 `vofa-next-core`,待 `can_types` / `logic_types` /
//! `diagnostic` crate 建立后再迁入本 crate。
//!
//! ## 设计原则
//!
//! 1. **单职责**:本 crate 仅承载跨域基础类型,**不依赖**任何 `protocol`/`buffer`/`nodes`/`automotive` 等。
//! 2. **serde 优先**:几乎所有类型派生 `Serialize`/`Deserialize`,便于与前端 IPC。
//! 3. **零业务**:仅数据载体,不包含协议解析/缓冲管理/调度逻辑。

pub mod error;
pub mod frame;
pub mod serial_params;

pub use error::{Error, Result};
pub use frame::{
    now_us, ConnectionState, DataFrame, PortInfo, RawData, TransportStats,
};
pub use serial_params::{FlowControl, Parity, StopBits};
