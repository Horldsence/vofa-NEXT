//! # can_types
//!
//! CAN 总线相关数据类型 + 缓冲区 + 负载统计。
//!
//! 模块拆分:
//! - [`can_frame`]: 帧/方向/波特率/过滤/批次/candle 设备
//! - [`can_buffer`]: CAN 帧环形缓冲区
//! - [`can_load_types`]: 负载统计快照与历史采样类型
//! - [`can_load_stats`]: 滑动时间窗负载统计器
//! - [`test_data`]: 测试数据生成工具
//!
//! ## 设计原则
//!
//! 1. **零依赖外部 crate**:仅依赖 [`vofa_core`]、`serde`。
//! 2. **serde 优先**:所有 wire 类型派生 `Serialize`/`Deserialize`,与前端 IPC。
//! 3. **单职责**:本 crate 不引入 `tokio` / `serialport` 等 I/O 依赖。

pub mod can_buffer;
pub mod can_frame;
pub mod can_load_stats;
pub mod can_load_types;
pub mod test_data;

pub use can_buffer::CanBuffer;
pub use can_frame::{
    CanBitrate, CanDirection, CanFilter, CanFrame, CanFrameBatch, CanFrameFilter, CandleDeviceInfo,
};
pub use can_load_stats::{frame_bits, CanLoadStats};
pub use can_load_types::{
    CanIdLoadHistory, CanIdLoadStats, CanLoadHistoryPoint, CanLoadSnapshot,
};
pub use test_data::CanFrameTestData;
