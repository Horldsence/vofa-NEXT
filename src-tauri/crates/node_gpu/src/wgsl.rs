//! WGSL 生成 — MathOp 链 → 直线 compute shader (一帧一线程)
//!
//! 布局: 上传矩阵 `in_mat` (行 = prelude 供给槽位并集, 列 = 帧), 下载矩阵
//! `out_mat` (行 = 单元输出槽位, 列 = 帧) — 线程 t 处理帧 t, 同槽位跨线程
//! 连续访存 (coalesced)。
//!
//! NaN 过滤语义与 [`node_kind::MathOp::evaluate`] 逐算子复刻: 先把非 NaN
//! 输入收进 `vals` 数组 (`x == x` 判 NaN), 再按算子归约 — 全 NaN/空输入
//! 一律 0.0 (含 Min/Max, 与 CPU 的 `vals.is_empty()` 提前返回一致)。

use std::collections::BTreeMap;
use std::fmt::Write;

use node_kind::MathOp;

use crate::elig::{GpuMathOp, GpuUnitPlan};

/// 生成单元 kernel 的 WGSL 模块源
///
/// * `in_pos`  - prelude 供给槽位 → 上传矩阵行 (图级并集下标; 会话构建期计算)
/// * `out_pos` - 输出槽位 → 下载矩阵行 (图级并集下标; 多单元同图共享矩阵,
///   行号必须按图级并集取, 否则单元间互相覆盖错行)
#[must_use]
pub fn emit_module(plan: &GpuUnitPlan, in_pos: &BTreeMap<u32, u32>, out_pos: &BTreeMap<u32, u32>) -> String {
    let k = plan.in_slots.len().max(1);
    let in_slots = fmt_slot_list(&plan.in_slots, k);
    let in_cols = {
        let cols: Vec<u32> = plan.in_slots.iter().map(|s| in_pos[s]).collect();
        fmt_slot_list(&cols, k)
    };
    let j = plan.out_slots.len().max(1);
    let out_slots = fmt_slot_list(&plan.out_slots, j);
    let out_rows = {
        let rows: Vec<u32> = plan.out_slots.iter().map(|s| out_pos[s]).collect();
        fmt_slot_list(&rows, j)
    };
    let ops: String = plan
        .math_ops
        .iter()
        .map(|op| emit_op(op, plan.max_arity))
        .collect();

    format!(
        r"// vofa 数值平面 math 单元 — gi {gi} / unit {unit_index}
// 帧间独立 (无状态 Math), 一帧一线程; NaN 过滤与 MathOp::evaluate 逐算子等价
struct Params {{
    n: u32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
}}

@group(0) @binding(0) var<storage, read> in_mat: array<f32>;
@group(0) @binding(1) var<storage, read_write> out_mat: array<f32>;
@group(0) @binding(2) var<uniform> params: Params;

const IN_SLOTS = array<u32, {k}u>({in_slots});
const IN_COLS = array<u32, {k}u>({in_cols});
const OUT_SLOTS = array<u32, {j}u>({out_slots});
const OUT_ROWS = array<u32, {j}u>({out_rows});
const SLOT_SPAN: u32 = {span}u;

fn trig_arg(x: f32) -> f32 {{
    // 2π 范围约减 — Metal/DX12 快速三角对大参数无精度保证; f32 约减在
    // |x| < 1e4 内误差 ~1e-6 rad, 远小于 ulp 容差契约
    return x - 6.2831855 * trunc(x * 0.15915494);
}}

@compute @workgroup_size({wg}u)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {{
    let t = gid.x;
    if (t >= params.n) {{
        return;
    }}
    var slots: array<f32, SLOT_SPAN>;
    for (var i: u32 = 0u; i < SLOT_SPAN; i = i + 1u) {{
        slots[i] = 0.0;
    }}
    for (var s: u32 = 0u; s < {k}u; s = s + 1u) {{
        slots[IN_SLOTS[s]] = in_mat[IN_COLS[s] * params.n + t];
    }}
{ops}
    for (var o: u32 = 0u; o < {j}u; o = o + 1u) {{
        out_mat[OUT_ROWS[o] * params.n + t] = slots[OUT_SLOTS[o]];
    }}
}}
",
        gi = plan.gi,
        unit_index = plan.unit_index,
        k = k,
        in_slots = in_slots,
        in_cols = in_cols,
        j = j,
        out_slots = out_slots,
        out_rows = out_rows,
        span = plan.slot_span,
        wg = crate::consts::WORKGROUP_SIZE,
        ops = ops.trim_end(),
    )
}

/// 槽位表 → WGSL 常量数组实参 (`1u, 2u, 3u`; 零长占位 1 个 0u)
fn fmt_slot_list(slots: &[u32], min_len: usize) -> String {
    if slots.is_empty() {
        return "0u".to_string();
    }
    let body: Vec<String> = slots.iter().map(|s| format!("{s}u")).collect();
    let mut s = body.join(", ");
    if min_len > slots.len() {
        s.push_str(", 0u");
    }
    s
}

/// 单 op → 收集 + 归约代码块 (块作用域隔离 cnt/vals/acc)
fn emit_op(op: &GpuMathOp, arity: u32) -> String {
    let mut collect = String::new();
    for (i, input) in op.inputs.iter().enumerate() {
        // 未连接 = 常量 0.0 (非 NaN, 直接纳入)
        let expr = input
            .as_ref()
            .map_or_else(|| "0.0".to_string(), |slot| format!("slots[{slot}u]"));
        // NaN 位模式检测 (指数全 1 + 尾数非 0, 非则收集) — Metal 默认快浮点
        // 会假设无 NaN 并把 `x == x` 折叠为 true, 位运算不受影响
        let _ = writeln!(
            collect,
            "        let x{i} = {expr};\n\
             \x20       let b{i} = bitcast<u32>(x{i});\n\
             \x20       if ((b{i} & 0x7f800000u) != 0x7f800000u || (b{i} & 0x007fffffu) == 0u) {{\n\
             \x20           vals[cnt] = x{i};\n\
             \x20           cnt = cnt + 1u;\n\
             \x20       }}"
        );
    }
    let o = op.out;
    let reduce = match op.op {
        MathOp::Add => fold_full(0.0, "acc + vals[i]", o),
        MathOp::Mul => fold_full(1.0, "acc * vals[i]", o),
        MathOp::Avg => format!(
            "if (cnt == 0u) {{\n            slots[{o}u] = 0.0;\n        }} else {{\n            var acc: f32 = 0.0;\n            {acc_loop}slots[{o}u] = acc / f32(cnt);\n        }}\n",
            acc_loop = fold_loop("acc + vals[i]"),
        ),
        MathOp::Sub => fold_from_first("acc - vals[i]", o),
        MathOp::Div => format!(
            "if (cnt == 0u) {{\n            slots[{o}u] = 0.0;\n        }} else {{\n            var acc: f32 = vals[0];\n            for (var i: u32 = 1u; i < cnt; i = i + 1u) {{\n                acc = select(acc / vals[i], 0.0, vals[i] == 0.0);\n            }}\n            slots[{o}u] = acc;\n        }}\n"
        ),
        MathOp::Min => fold_minmax(true, o),
        MathOp::Max => fold_minmax(false, o),
        MathOp::Abs => unary("abs(v0)", o),
        MathOp::Neg => unary("-v0", o),
        MathOp::Square => unary("v0 * v0", o),
        MathOp::Sqrt => unary("select(sqrt(v0), 0.0, v0 < 0.0)", o),
        MathOp::Sin => unary("sin(trig_arg(v0))", o),
        MathOp::Cos => unary("cos(trig_arg(v0))", o),
        MathOp::Tan => unary("tan(trig_arg(v0))", o),
        MathOp::Log => unary("select(log(v0), 0.0, v0 <= 0.0)", o),
    };
    format!("    {{\n        var cnt: u32 = 0u;\n        var vals: array<f32, {arity}u>;\n{collect}        {reduce}\n    }}\n")
}

/// 全量归约 (Add/Mul): 初值 + 全元素循环, 空集返回初值 (CPU unwrap_or 语义)
fn fold_full(init: f32, body: &str, o: u32) -> String {
    let init = if init == 0.0 { "0.0" } else { "1.0" };
    format!(
        "var acc: f32 = {init};\n        {loop_}slots[{o}u] = acc;\n",
        loop_ = fold_loop(body),
    )
}

/// 从首元素折叠 (Sub): 空集 0.0, 否则 vals[0] 起始
fn fold_from_first(body: &str, o: u32) -> String {
    format!(
        "if (cnt == 0u) {{\n            slots[{o}u] = 0.0;\n        }} else {{\n            var acc: f32 = vals[0];\n            for (var i: u32 = 1u; i < cnt; i = i + 1u) {{\n                acc = {body};\n            }}\n            slots[{o}u] = acc;\n        }}\n"
    )
}

/// Min/Max: 空集 0.0 (CPU vals.is_empty() 提前返回); 非空以 ±inf 起始折叠
fn fold_minmax(is_min: bool, o: u32) -> String {
    let (init, f) = if is_min {
        ("0x7f800000u", "min")
    } else {
        ("0xff800000u", "max")
    };
    format!(
        "if (cnt == 0u) {{\n            slots[{o}u] = 0.0;\n        }} else {{\n            var acc: f32 = bitcast<f32>({init});\n            for (var i: u32 = 0u; i < cnt; i = i + 1u) {{\n                acc = {f}(acc, vals[i]);\n            }}\n            slots[{o}u] = acc;\n        }}\n"
    )
}

/// 求和/求积循环体 (全量归约用)
fn fold_loop(body: &str) -> String {
    format!("for (var i: u32 = 0u; i < cnt; i = i + 1u) {{\n            acc = {body};\n        }}\n        ")
}

/// 一元算子 — v0 = 首个非 NaN 输入 (空集 0.0)
fn unary(expr: &str, o: u32) -> String {
    format!(
        "var v0: f32 = 0.0;\n        if (cnt > 0u) {{\n            v0 = vals[0];\n        }}\n        slots[{o}u] = {expr};\n"
    )
}
