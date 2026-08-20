//! # buffer_databuffer
//!
//! 多通道时间序列数据缓冲区 — 多通道 f32 + 时间戳 + 派生通道。
//!
//! - [`DataBuffer`]: 多通道 f32 时间序列, 自动通道扩展, 版本号单调递增
//! - 派生通道: 与主时间戳轴对齐, 索引直写零哈希 (批内高通量)
//! - [`WaveformWindow`]: 查询 API (get_recent / get_window), 派生通道 NaN 对齐
//!
//! 多数据源场景由 app 侧每源一个实例实现 (派生键 (sink, source) 随实例天然隔离)。

mod data_buffer;
mod derived;
mod window;

pub use data_buffer::DataBuffer;
pub use window::WaveformWindow;