//! 逻辑分析仪解码引擎
//!
//! - `core`: 引擎核心实现 (状态机 + ProtocolEngine trait)
//! - `pipeline`: UART / I2C / SPI 协议解码

pub mod core;
pub mod pipeline;

pub use core::LogicDecoderEngine;
