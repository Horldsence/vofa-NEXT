//! wgpu 设备上下文 — 平台后端选择 + 惰性全局单例
//!
//! compute 专用 (无 surface)。初始化一次, 失败结果同样缓存 — 无适配器的
//! 机器不反复探测 (request_adapter 在部分平台是数十毫秒级操作)。

use std::sync::Arc;
use std::sync::OnceLock;

/// 适配器信息 — 诊断与状态展示用
#[derive(Debug, Clone)]
pub struct GpuDeviceInfo {
    /// 后端名称 ("Metal" / "Vulkan" / "Dx12")
    pub backend: String,
    /// 适配器名称 (如 "Apple M2 Pro")
    pub name: String,
    /// 单次 dispatch 最大 workgroup 数 (x 维)
    pub max_workgroups_x: u32,
    /// 单个存储缓冲绑定最大字节数
    pub max_storage_buffer_binding_size: u64,
}

/// wgpu 设备上下文 — 计算管线宿主 (`device` + `queue` 内部 Arc, 克隆廉价)
pub struct GpuContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    info: GpuDeviceInfo,
}

/// 全局单例 — `None` 同样缓存 (无适配器机器不反复探测)
static CONTEXT: OnceLock<Option<Arc<GpuContext>>> = OnceLock::new();

impl GpuContext {
    /// 全局惰性单例 — 按平台主后端 ([`wgpu::Backends::PRIMARY`]) 初始化
    pub fn acquire() -> Option<Arc<Self>> {
        Self::acquire_with(wgpu::Backends::PRIMARY)
    }

    /// 指定后端集初始化 — 测试可注入 (如强制 GL/WARP); 同样走全局缓存
    pub fn acquire_with(backends: wgpu::Backends) -> Option<Arc<Self>> {
        CONTEXT.get_or_init(|| Self::init(backends)).clone()
    }

    /// 初始化 — 失败原因记日志后缓存 `None`
    fn init(backends: wgpu::Backends) -> Option<Arc<Self>> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .map_err(|e| log::warn!("GPU 适配器请求失败: {e}"))
        .ok()?;
        let (device, queue) =
            match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("vofa-gpu"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::default(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            })) {
                Ok(pair) => pair,
                Err(e) => {
                    log::warn!("GPU 设备请求失败: {e}");
                    return None;
                }
            };
        let info = adapter.get_info();
        let limits = device.limits();
        log::info!(
            "GPU 后端就绪: {:?} / {} (max_storage_binding = {} bytes)",
            info.backend,
            info.name,
            limits.max_storage_buffer_binding_size
        );
        Some(Arc::new(Self {
            info: GpuDeviceInfo {
                backend: format!("{:?}", info.backend),
                name: info.name,
                max_workgroups_x: limits.max_compute_workgroups_per_dimension,
                max_storage_buffer_binding_size: limits.max_storage_buffer_binding_size,
            },
            device,
            queue,
        }))
    }

    /// 设备 (创建管线 / 缓冲 / 编码器用)
    pub const fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// 命令队列 (上传 / 提交)
    pub const fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// 适配器信息
    pub const fn info(&self) -> &GpuDeviceInfo {
        &self.info
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 无 GPU 环境 (CI) 下 acquire 应返回 None 而非 panic
    #[test]
    fn 无适配器时优雅返回none() {
        // 不断言具体结果 (本地有 GPU / CI 无 GPU), 只验证不 panic、结果可重复
        let first = GpuContext::acquire();
        let second = GpuContext::acquire();
        assert_eq!(first.is_some(), second.is_some());
    }
}
