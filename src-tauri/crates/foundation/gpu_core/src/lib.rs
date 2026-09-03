//! GPU 基础设施 — wgpu 设备上下文 / 缓冲复用 / 包络降采样 / 错误类型
//!
//! 职责边界: 本 crate 只封装跨平台 compute 能力 (设备选择、缓冲生命周期、
//! 阻塞回读), **不懂数据平面与节点图结构**。图求值已回归 CPU (SciRS2 SIMD,
//! 见 `data_plane`); GPU 仅用于波形包络降采样 ([`envelope`]) —
//! 大窗口 min/max 逐列压缩, 前端 uPlot 仍负责绘制。
//!
//! 平台后端: macOS = Metal, Windows = DX12 + Vulkan, Linux = Vulkan
//! ([`wgpu::Backends::PRIMARY`])。无适配器时调用方优雅回退 CPU 路径。

pub use device::{GpuContext, GpuDeviceInfo};
pub use envelope::{envelope_minmax, envelope_minmax_cpu, Envelope};
pub use error::{GpuError, GpuResult};
pub use pool::{GpuBuffer, ReadbackStaging};

/// wgpu 再导出 — 上层 crate 经此取用, 工作区内 wgpu 版本单一收口
pub use wgpu;

mod device;
mod envelope;
mod error;
mod pool;
