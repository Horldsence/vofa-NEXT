//! CompiledEval 访问器 + 帧源预解析 ([`ResolvedFrames`])

use lower::{CompiledOp, SlotPlan};
use vofa_core::DataFrame;

use super::{CompiledEval, SourceFramesMap};

impl CompiledEval {
    /// 封装值平面 lowering 产物 — 编译 facade 流水线的第 3 段装配点
    pub const fn new(plan: SlotPlan) -> Self {
        Self { plan }
    }

    /// 槽位数 (调用方据此分配 slots/written 缓冲并跨帧复用)
    pub const fn slot_count(&self) -> usize {
        self.plan.slot_names.len()
    }

    /// 字符串槽位数 (调用方据此分配 str_slots/str_written 缓冲并跨帧复用)
    pub const fn str_slot_count(&self) -> usize {
        self.plan.str_slot_names.len()
    }

    /// TextOut 发送规格表 (发送 ticker / 手动命令的消费入口)
    pub fn textouts(&self) -> &[lower::TextOutSpec] {
        &self.plan.textouts
    }

    /// 平坦操作序列只读视图 — 编译期结构断言/诊断用
    pub fn ops(&self) -> &[CompiledOp] {
        &self.plan.ops
    }

    /// (node_id, port) → 槽位 (派生边批首解析用)
    pub fn slot_of(&self, node: &str, port: &str) -> Option<usize> {
        self.plan
            .slot_index
            .get(&(node.to_string(), port.to_string()))
            .copied()
    }

    /// (node_id, port) → 字符串槽位 (字符串输出发布解析用)
    pub fn str_slot_of(&self, node: &str, port: &str) -> Option<usize> {
        self.plan
            .str_slot_index
            .get(&(node.to_string(), port.to_string()))
            .copied()
    }

    /// 评估单元表 (下标 0 = prelude; 写集互斥, 见 `lower::EvalUnit`)
    pub fn units(&self) -> &[lower::EvalUnit] {
        &self.plan.units
    }

    /// 正本槽位 i → 所属单元下标 (运行时按单元→桶分派读路径)
    pub fn slot_unit(&self) -> &[u32] {
        &self.plan.slot_unit
    }

    /// ProtocolSource 引用的全局 Protocol 节点 id 表 (并发路径解析触发源下标用)
    pub fn frame_sources(&self) -> &[String] {
        &self.plan.frame_sources
    }

    /// Fft 输入槽位表: (sink_node_id, 源值槽位; None = 无上游边)
    pub fn spectrum_slots(&self) -> &[(String, Option<usize>)] {
        &self.plan.spectrum_slots
    }

    /// 静态本地图判定 — 纯外部常量输入的纯函数图:
    /// 无 Fft / TextOut, op 全部 ∈ {Input, TextInput, Custom, Math, Str}
    /// (无 ProtocolSource/Filter/Ifft/Trigger/FrameDecoder — 逐帧评估是纯浪费,
    /// 每批评估一次输出值相同; 见 graph_eval 静态图优化)
    pub fn is_static_local(&self) -> bool {
        self.plan.spectrum_slots.is_empty()
            && self.plan.textouts.is_empty()
            && self.plan.ops.iter().all(|op| {
                matches!(
                    op,
                    CompiledOp::Input { .. }
                        | CompiledOp::TextInput { .. }
                        | CompiledOp::Custom { .. }
                        | CompiledOp::Math { .. }
                        | CompiledOp::Str { .. }
                )
            })
    }

    /// 帧源预解析 — 每源一次字符串查找; `override_frame` 覆盖指定源
    /// 的帧引用 (并发路径: 触发源直接读批内帧切片, 不经共享缓存)
    pub fn resolve_frames<'a>(
        &'a self,
        source_frames: &'a SourceFramesMap,
        override_frame: Option<(usize, &'a DataFrame)>,
    ) -> ResolvedFrames<'a> {
        let n = self.plan.frame_sources.len();
        let mut stack = [None; 8];
        let mut heap: Vec<Option<&'a DataFrame>> = Vec::new();
        if n <= 8 {
            for (i, id) in self.plan.frame_sources.iter().enumerate() {
                stack[i] = source_frames.get(id);
            }
        } else {
            heap = self
                .plan
                .frame_sources
                .iter()
                .map(|id| source_frames.get(id))
                .collect();
        }
        let mut resolved = ResolvedFrames {
            stack,
            heap,
            len: n,
        };
        if let Some((idx, frame)) = override_frame {
            resolved.set(idx, frame);
        }
        resolved
    }
}

/// 预解析帧引用表 — 8 源以内走栈零分配, 超出落堆 (见 [`CompiledEval::resolve_frames`])
pub struct ResolvedFrames<'a> {
    stack: [Option<&'a DataFrame>; 8],
    heap: Vec<Option<&'a DataFrame>>,
    len: usize,
}

impl<'a> ResolvedFrames<'a> {
    fn set(&mut self, idx: usize, frame: &'a DataFrame) {
        if idx >= self.len {
            return;
        }
        if self.heap.is_empty() {
            self.stack[idx] = Some(frame);
        } else {
            self.heap[idx] = Some(frame);
        }
    }

    /// op 直读视图 (长度 == frame_sources 数)
    pub fn as_slice(&self) -> &[Option<&DataFrame>] {
        let s: &[Option<&DataFrame>] = if self.heap.is_empty() {
            &self.stack
        } else {
            &self.heap
        };
        &s[..self.len]
    }
}
