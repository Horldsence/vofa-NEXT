//! 后端低阶 (lowering) 驱动 — 值平面 MIR → 槽位产物 ([`SlotPlan`])
//!
//! 结构: [`SlotArena`] 槽位分配器 (f32/字符串各一实例) + 按节点种类分派的
//! per-kind lowering (见 `kinds` 模块)。
//! 输入边在编译期经 [`ValueMir`] 反查索引解析为槽位下标;
//! 查不到 = 常量 0.0 (与慢路径 resolve_input 缺省语义一致, 以 None 表示)。
//!
//! 评估单元切分 (见 `units` 模块): 供给节点 op 归入 prelude 单元 (单元 0),
//! 计算节点按连通分量成单元; 扁平 op 序按 [prelude][单元 1][单元 2].. 重排为
//! 连续区段 — 单元内保持拓扑序, 单元间无数据依赖 (跨单元读只指向 prelude 槽位)。

use rustc_hash::FxHashMap;

use kind::NodeKind;

use hir::TypedGraph;
use plane::ValueMir;

use crate::ops::{op_weight, CompiledOp, EvalUnit, TextOutSpec};
use crate::units;

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

    /// 已分配槽位数 (驱动按 mark..len 收集本次发射新分配的槽位)
    pub const fn len(&self) -> usize {
        self.names.len()
    }

    /// 是否未分配任何槽位
    pub const fn is_empty(&self) -> bool {
        self.names.is_empty()
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

/// lowering 上下文 — 输入反查 + 双 arena + 按单元分桶的操作序列 + 帧源表
///
/// `ops[cur_unit]` 为当前发射目标; kinds 经 [`LowerCtx::push_op`] 追加,
/// 驱动在访问每个节点前设置 `cur_unit` (供给 = 0 prelude, 计算 = 其分量单元)。
pub struct LowerCtx<'a> {
    pub mir: &'a ValueMir,
    pub f32_slots: SlotArena,
    pub str_slots: SlotArena,
    /// 每单元一个 op 序列 (单元内保持拓扑序; 下标 0 = prelude)
    pub ops: Vec<Vec<CompiledOp>>,
    /// 当前发射单元下标
    pub cur_unit: usize,
    /// ProtocolSource 帧源表 (去重): node_id → frame_sources 下标
    pub frame_sources: Vec<String>,
    /// TextOut 发送规格 (编译期收集, 供发送 ticker / 手动命令)
    pub textouts: Vec<TextOutSpec>,
}

impl LowerCtx<'_> {
    /// 当前单元追加一个 op
    pub fn push_op(&mut self, op: CompiledOp) {
        self.ops[self.cur_unit].push(op);
    }

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

/// 编译后端产物 — 平坦操作序列 + 双域槽位表 + 帧源表 + 评估单元表
///
/// 由 `eval` 封装为逐帧评估的 `CompiledEval`; 字段均为 lowering 直接产物。
pub struct SlotPlan {
    /// 槽位 i 对应的 (node_id, port) — 供快照物化/派生边反查
    pub slot_names: Vec<(String, String)>,
    /// (node_id, port) → 槽位下标
    pub slot_index: FxHashMap<(String, String), usize>,
    /// 平坦操作序列 — [prelude][单元 1][单元 2].. 连续区段 (见 `units`)
    pub ops: Vec<CompiledOp>,
    /// 评估单元表 (下标 0 = prelude; 写集互斥)
    pub units: Vec<EvalUnit>,
    /// 正本槽位 i → 所属单元下标 (运行时按单元→桶分派派生边/端口批/频谱读)
    pub slot_unit: Vec<u32>,
    /// SpectrumSink 输入槽位: (sink_node_id, 源值槽位; None = 无上游边, 与缺省 0.0 对应)
    pub spectrum_slots: Vec<(String, Option<usize>)>,
    /// ProtocolSource 引用的全局 Protocol 节点 id 表 (去重, 编译期预排;
    /// 逐帧评估时每源一次字符串查找解析为帧引用, op 用下标直读)
    pub frame_sources: Vec<String>,
    /// 字符串槽位 i 对应的 (node_id, port) — Str 节点 String 域输出, 仿 slot_names
    pub str_slot_names: Vec<(String, String)>,
    /// (node_id, port) → 字符串槽位下标
    pub str_slot_index: FxHashMap<(String, String), usize>,
    /// TextOut 发送规格表 — 发送 ticker / 手动命令的消费入口
    pub textouts: Vec<TextOutSpec>,
}

/// 值平面 lowering: 遍历拓扑序按节点 kind 分配输出槽位 + 生成平坦操作序列
///
/// op 按评估单元分桶收集 (供给 → prelude, 计算 → 分量单元), 最终扁平化为
/// [prelude][单元 1].. 连续区段; 槽位分配序不受分桶影响 (与旧版逐字节一致)。
pub fn lower_value_plane(g: &TypedGraph, mir: &ValueMir) -> SlotPlan {
    let unit_of = units::partition(g, mir);
    let unit_count = unit_of.iter().copied().max().unwrap_or(0) as usize + 1;

    let mut ctx = LowerCtx {
        mir,
        f32_slots: SlotArena::new(),
        str_slots: SlotArena::new(),
        ops: (0..unit_count).map(|_| Vec::new()).collect(),
        cur_unit: 0,
        frame_sources: Vec::new(),
        textouts: Vec::new(),
    };

    // 每单元写集 (清零表) + 正本槽位 → 单元映射
    let mut clear_f32: Vec<Vec<u32>> = vec![Vec::new(); unit_count];
    let mut clear_str: Vec<Vec<u32>> = vec![Vec::new(); unit_count];
    let mut slot_unit: Vec<u32> = Vec::new();
    let mut str_slot_unit: Vec<u32> = Vec::new();

    for (pos, &ix) in mir.order.iter().enumerate() {
        let Some(node) = g.graph[ix].value_def.as_ref() else {
            continue;
        };
        let u = unit_of[pos] as usize;
        ctx.cur_unit = u;
        let f32_mark = ctx.f32_slots.len();
        let str_mark = ctx.str_slots.len();
        crate::kinds::lower_node(node, &mut ctx);
        for s in f32_mark..ctx.f32_slots.len() {
            debug_assert_eq!(
                slot_unit.len(),
                s,
                "arena 槽位下标必须连续递增 (alloc 序 == slot idx 序)"
            );
            let slot = u32::try_from(s).unwrap_or(u32::MAX);
            slot_unit.push(u32::try_from(u).unwrap_or(u32::MAX));
            clear_f32[u].push(slot);
        }
        for s in str_mark..ctx.str_slots.len() {
            debug_assert_eq!(str_slot_unit.len(), s, "字符串槽位下标必须连续递增");
            let slot = u32::try_from(s).unwrap_or(u32::MAX);
            str_slot_unit.push(u32::try_from(u).unwrap_or(u32::MAX));
            clear_str[u].push(slot);
        }
    }

    // SpectrumSink 输入槽位 (不在 eval_order, 输入端口固定 "in0")
    let mut spectrum_slots = Vec::new();
    for node in g.value_nodes() {
        if matches!(node.kind, NodeKind::SpectrumSink { .. }) {
            spectrum_slots.push((node.id.clone(), ctx.f32_in(&node.id, "in0")));
        }
    }

    // 扁平化: [prelude][单元 1].. 连续区段 (单元内拓扑序已由访问序保证)
    let mut ops = Vec::new();
    let mut eval_units = Vec::with_capacity(unit_count);
    for (u, mut unit_ops) in ctx.ops.into_iter().enumerate() {
        let op_start = u32::try_from(ops.len()).unwrap_or(u32::MAX);
        ops.append(&mut unit_ops);
        let op_len = u32::try_from(ops.len()).unwrap_or(u32::MAX) - op_start;

        // 状态 id + 权重: 扫描本单元 op 区段 (编译期一次)
        let mut filter_ids = Vec::new();
        let mut ifft_ids = Vec::new();
        let mut trigger_ids = Vec::new();
        let mut weight = 0u32;
        for op in &ops[op_start as usize..(op_start + op_len) as usize] {
            weight += op_weight(op);
            match op {
                CompiledOp::Filter { node_id, .. } => filter_ids.push(node_id.as_str().into()),
                CompiledOp::Ifft { node_id, .. } => ifft_ids.push(node_id.as_str().into()),
                CompiledOp::Trigger { node_id, .. } => trigger_ids.push(node_id.as_str().into()),
                _ => {}
            }
        }

        eval_units.push(EvalUnit {
            op_start,
            op_len,
            clear_slots: std::mem::take(&mut clear_f32[u]),
            clear_str_slots: std::mem::take(&mut clear_str[u]),
            filter_ids,
            ifft_ids,
            trigger_ids,
            weight,
        });
    }

    let (slot_names, slot_index) = ctx.f32_slots.into_parts();
    let (str_slot_names, str_slot_index) = ctx.str_slots.into_parts();
    SlotPlan {
        slot_names,
        slot_index,
        ops,
        units: eval_units,
        slot_unit,
        spectrum_slots,
        frame_sources: ctx.frame_sources,
        str_slot_names,
        str_slot_index,
        textouts: ctx.textouts,
    }
}
