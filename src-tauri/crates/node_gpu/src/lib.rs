//! 算子 → WGSL 编译与批执行 — 数值平面无状态 Math 单元的 GPU 卸载
//!
//! 职责边界: 依赖 `node_lower` 的编译产物类型 ([`node_lower::CompiledOp`] /
//! [`node_lower::EvalUnit`]) 做资格分析与 WGSL 生成, 依赖 `gpu_core` 做设备
//! 与缓冲管理; **不依赖运行时状态** (槽位缓冲/帧缓存由调用方准备)。
//!
//! v1 资格标准 (见 [`elig`]): 单元 op 区段全部为 `Math`, 输入要么由 prelude
//! 供给 (`slot_unit == 0`), 要么由本单元先前 op 写出。Filter/Ifft/Trigger/
//! FrameDecoder 等有状态算子留在 CPU 路径。
//!
//! 数值等价契约: 加/减/乘与 min/max/abs 等纯 ALU 是 IEEE 精确 (与 CPU 位级
//! 一致); 除法/均值 (Metal 非精确舍入, WGSL 允许 2.5 ULP) 与 sin/cos/tan/
//! log/sqrt 在 Metal/DX12 后端与 Rust 存在 ≤2.5 ulp 差异;
//! [`node_kind::MathOp::evaluate`] 的 NaN 过滤语义逐算子复刻 (见 `wgsl`)。

pub use elig::{plan_unit, GpuUnitPlan};
pub use kernel::GpuUnitKernel;
pub use runner::GpuSession;
pub use wgsl::emit_module;

pub(crate) mod consts {
    /// compute workgroup 尺寸 (一帧一线程, 64 = 2 wave on most GPUs)
    pub const WORKGROUP_SIZE: u32 = 64;
}

mod elig;
mod kernel;
mod runner;
mod wgsl;

#[cfg(test)]
mod codegen_tests;
#[cfg(test)]
mod gpu_math_equiv;
