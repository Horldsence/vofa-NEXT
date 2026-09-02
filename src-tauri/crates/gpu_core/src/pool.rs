//! 设备缓冲复用 — 容量只增不减 + staging 阻塞回读
//!
//! 批处理每块 (chunk) 的上传/下载矩阵尺寸稳定 (帧数 × 槽位数), 复用设备
//! 缓冲避免 per-chunk 分配抖动; 容量不足时整块重建 (每块先全量上传, 旧
//! 内容无需保留)。回读契约: `enqueue_copy` → 调用方 submit →
//! `map_blocking` → `read_f32` → `unmap`。

use crate::error::{GpuError, GpuResult};

/// 容量自动增长的设备缓冲 — 用途由 `usage` 决定 (上传矩阵 / 输出矩阵 / staging)
pub struct GpuBuffer {
    device: wgpu::Device,
    buffer: wgpu::Buffer,
    capacity: u64,
    usage: wgpu::BufferUsages,
    label: &'static str,
}

impl GpuBuffer {
    /// 空缓冲 (容量 0) — 首次 [`Self::ensure`] 分配
    pub fn new(device: &wgpu::Device, usage: wgpu::BufferUsages, label: &'static str) -> Self {
        Self {
            device: device.clone(),
            buffer: Self::alloc(device, 0, usage, label),
            capacity: 0,
            usage,
            label,
        }
    }

    fn alloc(
        device: &wgpu::Device,
        bytes: u64,
        usage: wgpu::BufferUsages,
        label: &'static str,
    ) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: bytes.max(256), // 0 尺寸缓冲无绑定意义; 256 起步对齐绑定点
            usage,
            mapped_at_creation: false,
        })
    }

    /// 容量不足时按 1.5 倍重建 (256 字节对齐; 上限检查由调用方对照 device limits)
    pub fn ensure(&mut self, bytes: u64) {
        if bytes <= self.capacity {
            return;
        }
        let grown = bytes
            .saturating_add(bytes / 2)
            .next_multiple_of(256)
            .max(bytes);
        self.buffer = Self::alloc(&self.device, grown, self.usage, self.label);
        self.capacity = grown;
    }

    /// 底层 wgpu 缓冲
    pub const fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    /// 当前字节容量
    pub const fn capacity(&self) -> u64 {
        self.capacity
    }
}

/// 下载 staging — device → MAP_READ 拷贝 + 阻塞映射回读
///
/// 封装 map_async + `poll(Wait)` 的标准阻塞回读样板, 供 spawn_blocking
/// 线程使用 (数据平面评估不在 tokio worker 上)。
pub struct ReadbackStaging {
    inner: GpuBuffer,
    mapped: bool,
}

impl ReadbackStaging {
    /// 空 staging — 首次 [`Self::ensure`] 分配
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            inner: GpuBuffer::new(
                device,
                wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                "vofa-gpu-readback",
            ),
            mapped: false,
        }
    }

    /// 容量不足时重建 (见 [`GpuBuffer::ensure`])
    pub fn ensure(&mut self, bytes: u64) {
        self.inner.ensure(bytes);
    }

    /// 编码 device → staging 拷贝 (不提交; 调用方 submit 后再 `map_blocking`)
    pub fn enqueue_copy(&self, encoder: &mut wgpu::CommandEncoder, src: &wgpu::Buffer, bytes: u64) {
        encoder.copy_buffer_to_buffer(src, 0, self.inner.buffer(), 0, bytes);
    }

    /// map_async + `poll(Wait)` — 阻塞至映射完成 (要求拷贝命令已 submit,
    /// 否则无在途任务时回调永不触发)
    pub fn map_blocking(&mut self, device: &wgpu::Device) -> GpuResult<()> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.inner
            .buffer()
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |res| {
                let _ = tx.send(res);
            });
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| GpuError::Map(format!("poll(Wait): {e}")))?;
        match rx.recv() {
            Ok(Ok(())) => {
                self.mapped = true;
                Ok(())
            }
            Ok(Err(e)) => Err(GpuError::Map(format!("map_async: {e}"))),
            Err(e) => Err(GpuError::Map(format!("map 回调通道: {e}"))),
        }
    }

    /// 从已映射 staging 读取 `out.len()` 个 f32 (布局原样拷出)
    ///
    /// # Panics
    /// 未先 [`Self::map_blocking`] (或已 unmap) 时由 wgpu 映射校验 panic
    pub fn read_f32(&self, out: &mut [f32]) {
        let bytes = self
            .inner
            .buffer()
            .slice(..)
            .get_mapped_range()
            .map_err(|e| GpuError::Map(format!("get_mapped_range: {e}")))
            .expect("read_f32 要求 staging 处于已映射状态");
        let need = out.len() * 4;
        for (i, chunk) in bytes[..need].chunks_exact(4).enumerate() {
            out[i] = f32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
    }

    /// 解除映射 (幂等; 每块回读后必须调用, 否则下块 map 失败)
    pub fn unmap(&mut self) {
        if self.mapped {
            self.inner.buffer().unmap();
            self.mapped = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 无 GPU 时构造路径不 panic (仅覆盖纯逻辑分支; 缓冲创建需设备)
    #[test]
    fn 错误类型格式化() {
        let e = GpuError::BufferOverflow {
            required: 1024,
            limit: 512,
        };
        assert!(e.to_string().contains("1024"));
    }
}
