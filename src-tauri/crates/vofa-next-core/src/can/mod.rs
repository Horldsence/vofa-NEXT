//! CAN 类型模块
//!
//! - `can_types`: 负载统计相关类型 (CanLoadStats, CanLoadSnapshot, etc.)
//! - `can_core`: 核心类型 (CanFrame, CanBuffer, CanDirection, etc.)
//! - `test_data`: 测试数据生成器

pub mod can_core;
pub mod can_types;
pub mod test_data;

pub use can_core::*;
pub use can_types::*;
pub use test_data::CanFrameTestData;
