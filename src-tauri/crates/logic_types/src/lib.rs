//! # logic_types
//!
//! 逻辑分析仪(Logic Analyzer)数据类型。
//!
//! 模块:
//! - [`types`]: 采样 (`LogicSample`)、事件 (`DecodedEvent`)、过滤条件、解码器配置
//! - [`buffers`][]: 环形缓冲区 (`LogicBuffer`、`DecodedBuffer`)
//!
//! ## 设计原则
//!
//! - **零 I/O 依赖**:不引入 tokio/serialport,纯数据结构
//! - **serde 优先**:全部 wire 类型派生 Serialize/Deserialize
//! - **依赖 `vofa_core`**:Parity/StopBits 在 vofa_core 中(基础串口参数)

pub mod buffers;
pub mod types;

pub use buffers::{DecodedBuffer, LogicBuffer};
pub use types::{
    DecodedEvent, DecodedEventBatch, DecodedEventFilter, I2cEvent, LogicBatch, LogicDecoderConfig,
    LogicSample, LogicSampleBatch, LogicSampleFilter,
};
