//! 后端低阶 (lowering) 驱动 — 值平面 MIR → 槽位评估表 ([`CompiledEval`])
//!
//! 结构: [`SlotArena`] 槽位分配器 (f32/字符串各一实例) + 按节点种类分派的
//! per-kind lowering (见 `lower_kinds` 模块)。
//! 输入边在编译期经 [`ValueMir`] 反查索引解析为槽位下标;
//! 查不到 = 常量 0.0 (与 resolve_input 缺省语义一致, 以 None 表示)。

use rustc_hash::FxHashMap;

use node_kind::NodeKind;

use crate::eval::CompiledEval;
use crate::hir::TypedGraph;
use crate::ops::CompiledOp;
use crate::plane::ValueMir;

/// 槽位表拆解产物 (names + index)
pub type SlotTable = (Vec<(String, String)>, FxHashMap<(String, String), usize>);

/// 槽位 arena — 输出槽位分配 (f32/字符串各一实例)
///
/// 同 (node, port) 重复分配复用既有槽位 (显式 dedup, 与 set_port 覆盖写语义一致)。
pub struct SlotArena {
    names: Vec<(String, String)>,
    index: FxHashMap<(String, String), usize>,
}

impl SlotArena {
    pub fn new() -> Self {
        Self {
            names: Vec::new(),
            index: FxHashMap::default(),
        }
    }

    /// 分配一个输出槽位 (同 (node, port) 复用既有槽位)
    pub fn alloc(&mut self, node: &str, port: &str) -> usize {
        let key = (node.to_string(), port.to_string());
        if let Some(&i) = self.index.get(&key) {
            return i;
        }
        let i = self.names.len();
        self.index.insert(key.clone(), i);
        self.names.push(key);
        i
    }

    /// (node, port) → 槽位下标 (未分配 = None)
    pub fn resolve(&self, node: &str, port: &str) -> Option<usize> {
        self.index
            .get(&(node.to_string(), port.to_string()))
            .copied()
    }

    /// 拆解为 CompiledEval 字段
    pub fn into_parts(self) -> SlotTable {
        (self.names, self.index)
    }
}

/// lowering 上下文 — 输入反查 + 双 arena + 操作序列 + 帧源表
pub struct LowerCtx<'a> {
    pub mir: &'a ValueMir,
    pub f32_slots: SlotArena,
    pub str_slots: SlotArena,
    pub ops: Vec<CompiledOp>,
    /// ProtocolSource 帧源表 (去重): node_id → frame_sources 下标
    pub frame_sources: Vec<String>,
}

impl LowerCtx<'_> {
    /// f32 输入边 (node_id, in_name) → 上游输出槽位
    /// (无边/无槽位 = None, 与 resolve_input 缺省 0.0 对应)
    pub fn f32_in(&self, node_id: &str, in_name: &str) -> Option<usize> {
        self.mir
            .input_index
            .get(node_id)
            .and_then(|ports| ports.get(in_name))
            .and_then(|(sn, sp)| self.f32_slots.resolve(sn, sp))
    }

    /// 字符串输入边 (node_id, in_name) → 上游字符串槽位
    /// (无边/无槽位 = None, 与缺省 "" 对应)
    pub fn str_in(&self, node_id: &str, in_name: &str) -> Option<usize> {
        self.mir
            .string_input_index
            .get(node_id)
            .and_then(|ports| ports.get(in_name))
            .and_then(|(sn, sp)| self.str_slots.resolve(sn, sp))
    }

    /// 帧源 interning: source_id → frame_sources 下标 (去重)
    pub fn frame_source(&mut self, source_id: &str) -> usize {
        self.frame_sources
            .iter()
            .position(|s| s == source_id)
            .unwrap_or_else(|| {
                self.frame_sources.push(source_id.to_string());
                self.frame_sources.len() - 1
            })
    }
}

/// 值平面 lowering: 遍历拓扑序按节点 kind 分配输出槽位 + 生成平坦操作序列
pub fn lower_value_plane(g: &TypedGraph, mir: &ValueMir) -> CompiledEval {
    let mut ctx = LowerCtx {
        mir,
        f32_slots: SlotArena::new(),
        str_slots: SlotArena::new(),
        ops: Vec::new(),
        frame_sources: Vec::new(),
    };

    for &ix in &mir.order {
        let Some(node) = g.graph[ix].value_def.as_ref() else {
            continue;
        };
        kinds::lower_node(node, &mut ctx);
    }

    // SpectrumSink 输入槽位 (不在 eval_order, 输入端口固定 "in0")
    let mut spectrum_slots = Vec::new();
    for node in g.value_nodes() {
        if matches!(node.kind, NodeKind::SpectrumSink { .. }) {
            spectrum_slots.push((node.id.clone(), ctx.f32_in(&node.id, "in0")));
        }
    }

    let (slot_names, slot_index) = ctx.f32_slots.into_parts();
    let (str_slot_names, str_slot_index) = ctx.str_slots.into_parts();
    CompiledEval {
        slot_names,
        slot_index,
        ops: ctx.ops,
        spectrum_slots,
        frame_sources: ctx.frame_sources,
        str_slot_names,
        str_slot_index,
    }
}

pub mod kinds;
