//! 批执行器 — 上传 → dispatch → 单次 submit → 阻塞回读
//!
//! 调用契约 (数据平面 GPU 路径, 运行于 spawn_blocking 线程):
//! `enqueue` (每图一次, 编码 dispatch + staging 拷贝) → `finish_chunk`
//! (单次 submit + 阻塞映射) → `read_out` (每图一次, 拷出并 unmap)。
//!
//! 矩阵布局 (slot-major): `in_mat[col * n + frame]` / `out_mat[row * n + frame]`,
//! 行 = [`GpuSession::graph_in_slots`] / 输出槽位并集, 列 = 帧 — 同槽位跨
//! 线程连续访存。
//!
//! 错误契约: 任一步 `Err` 后本会话视为不可复用, 调用方应丢弃并回退 CPU
//! (映射状态可能残留)。设备与队列线程安全 (wgpu 内部 Arc), 会话本身由
//! 调用方串行使用 (数据平面按源互斥)。

use std::collections::BTreeMap;
use std::sync::Arc;

use gpu_core::wgpu;
use gpu_core::{GpuBuffer, GpuContext, GpuError, GpuResult, ReadbackStaging};

use crate::elig::GpuUnitPlan;
use crate::kernel::{create_bind_group_layout, GpuUnitKernel};
use crate::wgsl::emit_module;

/// 单元执行体 — kernel + 绑定组 (首块 ensure 时构建)
struct UnitExec {
    kernel: GpuUnitKernel,
    bg: Option<wgpu::BindGroup>,
}

/// 单图 GPU 执行状态 (按图 tab_id 键控 — 图表迭代序跨批稳定)
struct GraphExec {
    id: String,
    /// 上传矩阵行序 = prelude 供给槽位并集 (升序, 列下标即位置)
    in_slots: Vec<u32>,
    /// 输出矩阵行数 = 输出槽位并集大小
    out_count: usize,
    units: Vec<UnitExec>,
    in_buf: GpuBuffer,
    out_buf: GpuBuffer,
    staging: ReadbackStaging,
    /// 上传矩阵 → f32 字节流 (queue.write_buffer 需 &[u8])
    upload: Vec<u8>,
    /// 绑定组构建时的缓冲容量 (容量或缺省态变化 → 全体重建)
    built_at: Option<(u64, u64)>,
}

impl GraphExec {
    /// 缓冲容量保障 — 缓冲重建后同步重建全部绑定组
    fn ensure(
        &mut self,
        device: &wgpu::Device,
        bgl: &wgpu::BindGroupLayout,
        n_frames: u32,
    ) -> GpuResult<()> {
        let in_bytes = u64::from(n_frames) * self.in_slots.len() as u64 * 4;
        let out_bytes = u64::from(n_frames) * self.out_count as u64 * 4;
        let need = in_bytes.max(out_bytes);
        let max_buffer = device.limits().max_buffer_size;
        if need > max_buffer {
            return Err(GpuError::BufferOverflow {
                required: need,
                limit: max_buffer,
            });
        }
        self.in_buf.ensure(in_bytes);
        self.out_buf.ensure(out_bytes);
        self.staging.ensure(out_bytes);

        let caps = (self.in_buf.capacity(), self.out_buf.capacity());
        if self.built_at != Some(caps) {
            let kernels: Vec<GpuUnitKernel> = std::mem::take(&mut self.units)
                .into_iter()
                .map(|u| u.kernel)
                .collect();
            let (in_buf, out_buf) = (&self.in_buf, &self.out_buf);
            self.units = kernels
                .into_iter()
                .map(|kernel| {
                    let bg = make_bg(device, bgl, in_buf, out_buf, kernel.params());
                    UnitExec {
                        kernel,
                        bg: Some(bg),
                    }
                })
                .collect();
            self.built_at = Some(caps);
        }
        Ok(())
    }

    /// 单图单块入队 (见 [`GpuSession::enqueue`])
    fn enqueue(
        &mut self,
        ctx: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        n_frames: u32,
        in_mat: &[f32],
    ) -> GpuResult<()> {
        let groups = n_frames.div_ceil(crate::consts::WORKGROUP_SIZE);
        let max_groups = ctx.device().limits().max_compute_workgroups_per_dimension;
        if groups > max_groups {
            return Err(GpuError::TooManyWorkgroups {
                required: groups,
                limit: max_groups,
            });
        }

        // 参数 (每单元 16B uniform) + 输入矩阵上传 (queue 内部完成暂存拷贝)
        let mut nbuf = [0u8; 16];
        nbuf[0..4].copy_from_slice(&n_frames.to_le_bytes());
        self.upload.clear();
        self.upload
            .extend(in_mat.iter().flat_map(|v| v.to_le_bytes()));
        for u in &self.units {
            ctx.queue().write_buffer(u.kernel.params(), 0, &nbuf);
        }
        ctx.queue()
            .write_buffer(self.in_buf.buffer(), 0, &self.upload);

        // dispatch 全部单元 (共享图级矩阵缓冲) + staging 拷贝编码
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("vofa-math-pass"),
            timestamp_writes: None,
        });
        for u in &self.units {
            let bg = u.bg.as_ref().expect("绑定组由 ensure 构建");
            pass.set_pipeline(u.kernel.pipeline());
            pass.set_bind_group(0, bg, &[]);
            pass.dispatch_workgroups(groups, 1, 1);
        }
        drop(pass);
        let out_bytes = u64::from(n_frames) * self.out_count as u64 * 4;
        self.staging
            .enqueue_copy(encoder, self.out_buf.buffer(), out_bytes);
        Ok(())
    }
}

fn make_bg(
    device: &wgpu::Device,
    bgl: &wgpu::BindGroupLayout,
    in_buf: &GpuBuffer,
    out_buf: &GpuBuffer,
    params: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("vofa-math-bg"),
        layout: bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: in_buf.buffer().as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: out_buf.buffer().as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: params.as_entire_binding(),
            },
        ],
    })
}

/// GPU 批执行会话 — 一图一执行状态, 全单元共享图级矩阵缓冲
///
/// 构建 ([`GpuSession::build`]) 期完成 WGSL 生成与管线编译; 每块仅做
/// 参数/输入上传 + dispatch 编码 + 一次 submit + 一次阻塞映射。
pub struct GpuSession {
    ctx: Arc<GpuContext>,
    bgl: wgpu::BindGroupLayout,
    graphs: Vec<GraphExec>,
    encoder: Option<wgpu::CommandEncoder>,
}

impl GpuSession {
    /// 构建会话 — `graphs`: (图 tab_id, 该图全部 GPU 资格单元计划)
    ///
    /// 任一单元编译失败整体 `Err`/panic (调用方回退 CPU, 不做部分 GPU 化)。
    pub fn build(ctx: Arc<GpuContext>, graphs: &[(String, Vec<Arc<GpuUnitPlan>>)]) -> Self {
        let device = ctx.device();
        let bgl = create_bind_group_layout(device);
        let mut execs = Vec::with_capacity(graphs.len());
        for (id, plans) in graphs {
            if plans.is_empty() {
                continue;
            }
            // 上传/下载矩阵行 = 槽位并集, 按**槽位升序**重新编号 — 调用方
            // (数据平面作业) 以排序槽位序写入矩阵, 两套编号必须一致
            let mut in_map: BTreeMap<u32, u32> = BTreeMap::new();
            let mut out_map: BTreeMap<u32, u32> = BTreeMap::new();
            for plan in plans {
                for &s in &plan.in_slots {
                    in_map.insert(s, 0);
                }
                for &s in &plan.out_slots {
                    out_map.insert(s, 0);
                }
            }
            for (i, (_, pos)) in in_map.iter_mut().enumerate() {
                *pos = u32::try_from(i).unwrap_or(u32::MAX);
            }
            for (i, (_, pos)) in out_map.iter_mut().enumerate() {
                *pos = u32::try_from(i).unwrap_or(u32::MAX);
            }
            let out_count = out_map.len();
            let units: Vec<UnitExec> = plans
                .iter()
                .map(|plan| {
                    let wgsl = emit_module(plan, &in_map, &out_map);
                    UnitExec {
                        kernel: GpuUnitKernel::new(device, &wgsl),
                        bg: None,
                    }
                })
                .collect();
            execs.push(GraphExec {
                id: id.clone(),
                in_slots: in_map.into_keys().collect(),
                out_count,
                units,
                in_buf: GpuBuffer::new(
                    device,
                    wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    "vofa-gpu-in",
                ),
                out_buf: GpuBuffer::new(
                    device,
                    wgpu::BufferUsages::STORAGE
                        | wgpu::BufferUsages::COPY_DST
                        | wgpu::BufferUsages::COPY_SRC,
                    "vofa-gpu-out",
                ),
                staging: ReadbackStaging::new(device),
                upload: Vec::new(),
                built_at: None,
            });
        }
        Self {
            ctx,
            bgl,
            graphs: execs,
            encoder: None,
        }
    }

    /// 图的上传矩阵行序 (槽位升序; 位置即列下标; 未注册图返回空)
    #[must_use]
    pub fn graph_in_slots(&self, id: &str) -> &[u32] {
        self.graphs
            .iter()
            .find(|g| g.id == id)
            .map_or(&[][..], |g| g.in_slots.as_slice())
    }

    /// 图的输出矩阵行数 (下载矩阵元素数 = 帧数 × 本值; 未注册图返回 0)
    #[must_use]
    pub fn graph_out_count(&self, id: &str) -> usize {
        self.graphs
            .iter()
            .find(|g| g.id == id)
            .map_or(0, |g| g.out_count)
    }

    /// 编码单图本块执行 — 上传参数/输入, dispatch 全部单元, 编码 staging 拷贝
    ///
    /// `in_mat` 长度契约: `n_frames × graph_in_slots(id).len()` (slot-major)。
    pub fn enqueue(&mut self, id: &str, n_frames: u32, in_mat: &[f32]) -> GpuResult<()> {
        if n_frames == 0 {
            return Ok(());
        }
        let device = self.ctx.device().clone();
        let g = self
            .graphs
            .iter_mut()
            .find(|g| g.id == id)
            .ok_or_else(|| GpuError::Map(format!("图 {id} 未注册 GPU 会话")))?;
        g.ensure(&device, &self.bgl, n_frames)?;
        let encoder = self.encoder.get_or_insert_with(|| {
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("vofa-gpu-chunk"),
            })
        });
        g.enqueue(&self.ctx, encoder, n_frames, in_mat)
    }

    /// 提交本块全部工作并阻塞等待 (映射各图 staging 供 [`Self::read_out`])
    pub fn finish_chunk(&mut self) -> GpuResult<()> {
        let Some(encoder) = self.encoder.take() else {
            return Ok(());
        };
        self.ctx.queue().submit(Some(encoder.finish()));
        let device = self.ctx.device().clone();
        for g in &mut self.graphs {
            g.staging.map_blocking(&device)?;
        }
        Ok(())
    }

    /// 拷出单图输出矩阵并解除映射 (`out.len()` = 帧数 × [`Self::graph_out_count`])
    pub fn read_out(&mut self, id: &str, out: &mut [f32]) {
        let Some(g) = self.graphs.iter_mut().find(|g| g.id == id) else {
            return;
        };
        g.staging.read_f32(out);
        g.staging.unmap();
    }
}
