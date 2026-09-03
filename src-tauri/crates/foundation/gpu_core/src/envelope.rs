//! 波形包络降采样 — 大窗口逐列 min/max 压缩 (GPU compute + CPU 参考)
//!
//! 「wgpu 预处理渲染波形, 前端只显示」论证结论的落地形态 (原型): 整帧像素
//! 流经 IPC 是负收益 (无零拷贝纹理共享 / 带宽超预算一个数量级 / 交互变
//! 往返), 而 GPU 擅长的逐点归约压缩 — N 点 → columns×(min, max, count) —
//! 让前端 uPlot 以缎带绘制全窗口包络, 数据量缩小 N/columns 倍。
//!
//! 数值约定:
//! - 列映射: `col(i) = floor((i + 0.5) * columns / n)` (f32 运算, CPU/GPU
//!   同式同精度 → 同列归属; 0.5 偏移让每列近似等宽)
//! - min/max 只统计非 NaN 样本; 空列 (无样本或全 NaN) min=+inf / max=-inf /
//!   count=0 — 前端按断线处理
//! - ±0 归一为 +0 (保序位键要求; 显示上 ±0 等同)
//! - GPU 原子 min/max 用有序浮点位键 (负数全位取反 / 非负仅翻符号位),
//!   与 CPU 线性扫描位级一致
//!
//! 原型边界: 每点两次全局原子操作, 未做 workgroup 内共享内存归约 —
//! 100k~4M 点量级已远超窗口推送预算, 进一步优化 (分层归约) 留待实测需要。

use crate::error::{GpuError, GpuResult};
use crate::pool::ReadbackStaging;

/// 逐列包络 — 长度均为 `columns` (空列: min=+inf / max=-inf / count=0)
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Envelope {
    /// 列内非 NaN 样本最小值 (空列 +inf)
    pub min: Vec<f32>,
    /// 列内非 NaN 样本最大值 (空列 -inf)
    pub max: Vec<f32>,
    /// 列内非 NaN 样本数
    pub count: Vec<u32>,
}

impl Envelope {
    /// 列是否为空 (无有效样本)
    #[must_use]
    pub fn is_empty_column(&self, col: usize) -> bool {
        self.count.get(col).is_none_or(|&c| c == 0)
    }
}

/// 列映射 — CPU/GPU 共用的 f32 同式实现 (见模块注释)
///
/// cast lint 有意豁免: usize→f32 截断与 f32→u32 转换是 CPU/GPU 位级一致的
/// 前提 (WGSL 侧同为 f32/u32 运算), 不可用更宽类型替代
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
#[must_use]
pub fn column_of(i: usize, n: usize, columns: usize) -> usize {
    let scale = columns as f32 / n as f32;
    let col = ((i as f32 + 0.5) * scale).floor();
    if col < 0.0 {
        0
    } else {
        usize::try_from(col as u32)
            .unwrap_or(columns - 1)
            .min(columns - 1)
    }
}

/// ±0 归一 — 保序位键的前置条件 (CPU/WGSL 同语义)
fn normalize(v: f32) -> f32 {
    if v == 0.0 {
        0.0
    } else {
        v
    }
}

/// CPU 参考实现 — 线性扫描 (GPU 不可用时的回退路径与等价性仲裁基准)
// cast 豁免理由同 [`column_of`]
#[allow(clippy::cast_precision_loss)]
#[must_use]
pub fn envelope_minmax_cpu(values: &[f32], columns: usize) -> Envelope {
    let mut out = Envelope {
        min: vec![f32::INFINITY; columns],
        max: vec![f32::NEG_INFINITY; columns],
        count: vec![0; columns],
    };
    let n = values.len();
    for (i, &v0) in values.iter().enumerate() {
        if v0.is_nan() {
            continue;
        }
        let v = normalize(v0);
        let col = column_of(i, n, columns);
        out.min[col] = out.min[col].min(v);
        out.max[col] = out.max[col].max(v);
        out.count[col] += 1;
    }
    out
}

/// 有序浮点位键 (编码) — 负数全位取反 / 非负仅翻符号位, 全域单调双射
///
/// NaN 必须先被排除; ±0 归一后 -0 不会进入本函数。解码用 [`decode_key`]。
const fn ordered_key(bits: u32) -> u32 {
    if bits & 0x8000_0000 != 0 {
        !bits
    } else {
        bits ^ 0x8000_0000
    }
}

/// 有序浮点位键 (解码) — [`ordered_key`] 的镜像: 编码后符号位为 0 表示原值
/// 为负 (全取反), 为 1 表示原值非负 (仅符号位被翻)
const fn decode_key(key: u32) -> u32 {
    if key & 0x8000_0000 != 0 {
        key ^ 0x8000_0000
    } else {
        !key
    }
}

const ENV_WGSL: &str = r"
struct Params {
    n: u32,
    columns: u32,
    scale: f32,
    pad: u32,
}

@group(0) @binding(0) var<storage, read> values: array<f32>;
@group(0) @binding(1) var<storage, read_write> acc_min: array<atomic<u32>>;
@group(0) @binding(2) var<storage, read_write> acc_max: array<atomic<u32>>;
@group(0) @binding(3) var<storage, read_write> acc_count: array<atomic<u32>>;
@group(0) @binding(4) var<uniform> params: Params;

fn ordered_key(bits: u32) -> u32 {
    return select(bits ^ 0x80000000u, ~bits, (bits & 0x80000000u) != 0u);
}

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= params.n) {
        return;
    }
    // NaN 位模式检测 — 不能用 `raw != raw` (Metal 快浮点会折叠为 false,
    // 见 git 历史 node_gpu/src/wgsl.rs 同款对策); 位运算不受快浮点影响
    let raw_bits = bitcast<u32>(values[i]);
    if ((raw_bits & 0x7f800000u) == 0x7f800000u && (raw_bits & 0x007fffffu) != 0u) {
        return;
    }
    let raw = values[i];
    let v = select(raw, 0.0, (raw_bits & 0x7fffffffu) == 0u); // ±0 位测试归一 (Metal 快浮点连 == 0.0 都不可靠)
    let col = min(u32(floor((f32(i) + 0.5) * params.scale)), params.columns - 1u);
    let key = ordered_key(bitcast<u32>(v));
    atomicMin(&acc_min[col], key);
    atomicMax(&acc_max[col], key);
    atomicAdd(&acc_count[col], 1u);
}
";

/// GPU uniform 参数 (scale 与 CPU 的 `columns as f32 / n as f32` 同式)
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    n: u32,
    columns: u32,
    scale: f32,
    pad: u32,
}

/// GPU 包络降采样 — N 点 → `columns`×(min, max, count)
///
/// 无适配器 / 设备失败时返回 `Err`, 调用方应回退 [`envelope_minmax_cpu`]。
// scale 的 usize→f32 截断是有意为之 (与 CPU 参考同式, 见模块注释)
#[allow(clippy::cast_precision_loss)]
pub fn envelope_minmax(
    ctx: &crate::GpuContext,
    values: &[f32],
    columns: usize,
) -> GpuResult<Envelope> {
    let device = ctx.device();
    if columns == 0 {
        return Err(GpuError::Map("columns 为 0".into()));
    }
    let n = values.len();
    let columns_u32 =
        u32::try_from(columns).map_err(|_| GpuError::Map("columns 超出 u32".into()))?;
    let n_u32 = u32::try_from(n).map_err(|_| GpuError::Map("样本数超出 u32".into()))?;

    // 输出缓冲 (映射期写初值 = 空列编码, 避免为一次性上传引入 util feature)
    let out_bytes = u64::from(columns_u32) * 4;
    let min_init = vec![ordered_key(0x7F80_0000); columns]; // key(+inf) = 0xFF80_0000
    let max_init = vec![ordered_key(0xFF80_0000); columns]; // key(-inf) = 0x007F_FFFF
    let make_out = |label: &'static str, init: &[u32]| {
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: out_bytes.max(256),
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: true,
        });
        let mut view = buf
            .slice(..out_bytes)
            .get_mapped_range_mut()
            .expect("映射期写入应成功");
        view.copy_from_slice(bytemuck::cast_slice(init));
        drop(view);
        buf.unmap();
        buf
    };
    let min_buf = make_out("env-min", &min_init);
    let max_buf = make_out("env-max", &max_init);
    let count_buf = make_out("env-count", &vec![0u32; columns]);

    // 输入缓冲 (n = 0 时仍按 256 字节占位, kernel 越界守卫保证不读)
    let values_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("env-values"),
        size: (u64::try_from(n * 4).unwrap_or(0)).max(256),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    if n > 0 {
        ctx.queue()
            .write_buffer(&values_buf, 0, bytemuck::cast_slice(values));
    }

    let params_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("env-params"),
        size: std::mem::size_of::<Params>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    ctx.queue().write_buffer(
        &params_buf,
        0,
        bytemuck::bytes_of(&Params {
            n: n_u32,
            columns: columns_u32,
            scale: columns as f32 / n.max(1) as f32,
            pad: 0,
        }),
    );

    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("env-wgsl"),
        source: wgpu::ShaderSource::Wgsl(ENV_WGSL.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("env-pipeline"),
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });

    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("env-bind"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: values_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: min_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: max_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: count_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: params_buf.as_entire_binding(),
            },
        ],
    });

    {
        // dispatch 独立提交 (wgpu 30 finish 拿走 encoder; 同队列提交序即执行序)
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("env-encoder"),
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("env-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind, &[]);
        // n = 0 也 dispatch 1 组 (kernel 首行守卫返回; dispatch_workgroups 拒绝 0)
        pass.dispatch_workgroups(
            u32::try_from(n.div_ceil(256)).unwrap_or(u32::MAX).max(1),
            1,
            1,
        );
        drop(pass);
        ctx.queue().submit(Some(encoder.finish()));
    }

    let mut staging = ReadbackStaging::new(device);
    let read_out = |staging: &mut ReadbackStaging,
                    src: &wgpu::Buffer,
                    columns: usize|
     -> GpuResult<Vec<u32>> {
        let bytes = u64::try_from(columns * 4).unwrap_or(0);
        staging.ensure(bytes);
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("env-readback"),
        });
        staging.enqueue_copy(&mut enc, src, bytes);
        ctx.queue().submit(Some(enc.finish()));
        staging.map_blocking(device)?;
        let mut out = vec![0u32; columns];
        staging.read_u32(&mut out);
        staging.unmap();
        Ok(out)
    };
    let min_keys = read_out(&mut staging, &min_buf, columns)?;
    let max_keys = read_out(&mut staging, &max_buf, columns)?;
    let count = read_out(&mut staging, &count_buf, columns)?;

    // 有序键 → f32; 空列解码回 ±inf
    let decode = |keys: &[u32]| {
        keys.iter()
            .map(|&k| f32::from_bits(decode_key(k)))
            .collect()
    };
    Ok(Envelope {
        min: decode(&min_keys),
        max: decode(&max_keys),
        count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 列映射: 单调不减、范围合法、0.5 偏移近似等宽
    #[test]
    fn 列映射单调且合法() {
        for (n, columns) in [
            (1000usize, 64usize),
            (17, 64),
            (64, 17),
            (1, 8),
            (100_003, 2000),
        ] {
            let mut prev = 0;
            for i in 0..n {
                let col = column_of(i, n, columns);
                assert!(col < columns, "n={n} cols={columns} i={i} 越界");
                assert!(col >= prev, "n={n} cols={columns} i={i} 非单调");
                prev = col;
            }
        }
        // n=1, columns=8: scale=8 → col(0) = floor(0.5 * 8) = 4
        assert_eq!(column_of(0, 1, 8), 4);
    }

    /// CPU 参考: 空输入 / 全 NaN / 列数大于样本数
    #[test]
    #[allow(clippy::float_cmp)] // 断言精确值 (手算常量, 非运算结果)
    fn cpu参考边界() {
        let e = envelope_minmax_cpu(&[], 4);
        assert!(e.min.iter().all(|v| *v == f32::INFINITY));
        assert!(e.count.iter().all(|&c| c == 0));

        let e = envelope_minmax_cpu(&[f32::NAN; 3], 2);
        assert!(e.count.iter().all(|&c| c == 0));

        let e = envelope_minmax_cpu(&[1.0, 2.0, f32::NAN, -3.0], 8);
        let total: u32 = e.count.iter().sum();
        assert_eq!(total, 3, "NaN 不计数");
        assert!(e.min.contains(&1.0));
    }

    /// CPU 参考: 手工可验小案例 (4 点 → 2 列, 精确列归属)
    #[test]
    #[allow(clippy::float_cmp)] // 断言精确值 (手算常量, 非运算结果)
    fn cpu参考手算() {
        // n=4, columns=2: scale = 0.5; col(i) = floor((i+0.5)/2)
        // i=0 → floor(0.25)=0; i=1 → floor(0.75)=0; i=2 → floor(1.25)=1; i=3 → floor(1.75)=1
        let e = envelope_minmax_cpu(&[5.0, 1.0, 7.0, 3.0], 2);
        assert_eq!(e.min, vec![1.0, 3.0]);
        assert_eq!(e.max, vec![5.0, 7.0]);
        assert_eq!(e.count, vec![2, 2]);
    }

    /// 有序键: 编解码往返 + 全域单调 (±0 / ±inf 边界)
    #[test]
    fn 有序键编解码单调() {
        // 往返: 覆盖正/负/次正规/极值 (NaN 除外 — 调用侧排除; -0 由归一排除)
        for &bits in &[
            0x0000_0000u32, // +0
            0x0000_0001,    // 最小正次正规
            0x3F80_0000,    // 1.0
            0x7F80_0000,    // +inf
            0x7F7F_FFFF,    // 最大有限正
            0x8000_0001,    // 最大幅度负次正规
            0xBF80_0000,    // -1.0
            0xFF7F_FFFF,    // 最小有限负
            0xFF80_0000,    // -inf
        ] {
            assert_eq!(decode_key(ordered_key(bits)), bits, "bits={bits:08x}");
        }
        // 单调: IEEE 全序 (值升序: -inf < 最大幅度负 < -1.0 < 最小幅度负 < +0 < …)
        let asc = [
            0xFF80_0000u32,
            0xFF7F_FFFF,
            0xBF80_0000,
            0x8000_0001,
            0x0000_0000,
            0x0000_0001,
            0x3F80_0000,
            0x7F7F_FFFF,
            0x7F80_0000,
        ];
        for pair in asc.windows(2) {
            assert!(
                ordered_key(pair[0]) < ordered_key(pair[1]),
                "{:08x} 应小于 {:08x}",
                pair[0],
                pair[1]
            );
        }
        // 空列初值解码: +inf / -inf
        assert_eq!(decode_key(ordered_key(0x7F80_0000)), 0x7F80_0000);
        assert_eq!(decode_key(ordered_key(0xFF80_0000)), 0xFF80_0000);

        // ±0 归一: min/max 里 -0 与 +0 等同
        let e = envelope_minmax_cpu(&[-0.0, 0.0], 1);
        assert_eq!(e.min[0].to_bits(), 0.0f32.to_bits());
        assert_eq!(e.max[0].to_bits(), 0.0f32.to_bits());
        assert_eq!(e.count[0], 2);
    }
}
