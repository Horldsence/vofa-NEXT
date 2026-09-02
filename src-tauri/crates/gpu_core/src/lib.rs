//! GPU 基础设施 — wgpu 设备上下文 / 缓冲复用 / 错误类型
//!
//! 职责边界: 本 crate 只封装跨平台 compute 能力 (设备选择、缓冲生命周期、
//! 阻塞回读), **不懂数据平面与节点图结构**。算子 → WGSL 的编译与批执行
//! 在 `node_gpu`, 集成调度在 `pipeline_data_plane`。
//!
//! 平台后端: macOS = Metal, Windows = DX12 + Vulkan, Linux = Vulkan
//! ([`wgpu::Backends::PRIMARY`])。无适配器时调用方优雅回退 CPU 路径。

pub use device::{GpuContext, GpuDeviceInfo};
pub use error::{GpuError, GpuResult};
pub use pool::{GpuBuffer, ReadbackStaging};

/// wgpu 再导出 — 上层 crate (node_gpu) 经此取用, 工作区内 wgpu 版本单一收口
pub use wgpu;

mod device;
mod error;
mod pool;
