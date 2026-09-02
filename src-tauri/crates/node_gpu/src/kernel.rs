//! kernel 编译 — WGSL 模块 → 计算管线 + 参数 uniform
//!
//! 资格分析与 codegen 测试已保证 WGSL 结构合法; 非法源会在模块创建时
//! 被 wgpu 校验 panic (fail-fast, 集成侧以 catch_unwind 回退 CPU 兜底)。

use std::borrow::Cow;

use gpu_core::wgpu;

/// 单元计算管线 — pipeline + 16 字节参数 uniform (n_frames + 填充)
pub struct GpuUnitKernel {
    pipeline: wgpu::ComputePipeline,
    params: wgpu::Buffer,
}

impl GpuUnitKernel {
    /// 编译 WGSL 源为计算管线 (每单元一次, 会话构建期完成)
    pub fn new(device: &wgpu::Device, wgsl: &str) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vofa-math-unit"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(wgsl)),
        });
        let bgl = create_bind_group_layout(device);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vofa-math-layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("vofa-math-pipeline"),
            layout: Some(&layout),
            module: &module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vofa-math-params"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { pipeline, params }
    }

    /// 计算管线
    pub const fn pipeline(&self) -> &wgpu::ComputePipeline {
        &self.pipeline
    }

    /// 参数 uniform (仅前 4 字节 = n_frames 被消费)
    pub const fn params(&self) -> &wgpu::Buffer {
        &self.params
    }
}

/// 绑定组布局 — 0: 输入矩阵 (只读 storage), 1: 输出矩阵 (读写 storage),
/// 2: 参数 uniform。全部单元同构, 每会话一份。
#[must_use]
pub fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let buffer = |read_only: bool, min: u64| wgpu::BindingType::Buffer {
        ty: if read_only {
            wgpu::BufferBindingType::Storage { read_only: true }
        } else {
            wgpu::BufferBindingType::Storage { read_only: false }
        },
        has_dynamic_offset: false,
        min_binding_size: wgpu::BufferSize::new(min),
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("vofa-math-bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: buffer(true, 4),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: buffer(false, 4),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(16),
                },
                count: None,
            },
        ],
    })
}
