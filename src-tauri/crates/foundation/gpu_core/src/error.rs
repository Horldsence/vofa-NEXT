//! GPU 错误类型 — 统一 [`GpuResult`], 调用方据此回退 CPU 路径

/// GPU 计算错误 — 设备初始化失败 / 缓冲越界 / 映射失败
#[derive(Debug, thiserror::Error)]
pub enum GpuError {
    /// 无可用适配器 (无 GPU / 驱动不可用 / 后端未编译)
    #[error("无可用 GPU 适配器 (backends = {backends:?})")]
    NoAdapter {
        /// 尝试过的后端集合
        backends: wgpu::Backends,
    },
    /// 设备请求失败 (limits 不可满足等)
    #[error("GPU 设备请求失败: {0}")]
    Device(String),
    /// 容量请求超过设备上限
    #[error("GPU 缓冲区容量超限: 需要 {required} 字节, 设备上限 {limit}")]
    BufferOverflow {
        /// 需要的字节数
        required: u64,
        /// 设备允许的最大字节数
        limit: u64,
    },
    /// staging 映射 / 回读失败
    #[error("GPU 缓冲区映射失败: {0}")]
    Map(String),
    /// dispatch workgroup 数超设备上限
    #[error("GPU dispatch 超限: 需 {required} 个 workgroup, 设备上限 {limit}")]
    TooManyWorkgroups {
        /// 需要的 workgroup 数
        required: u32,
        /// 设备单维上限
        limit: u32,
    },
}

/// GPU 操作结果
pub type GpuResult<T> = Result<T, GpuError>;
