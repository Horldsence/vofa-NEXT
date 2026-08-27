//! 图编译流水线 facade — HIR → 平面 MIR → 后端产物
//!
//! 编译流程 (三段式, 对应编译器前端/中端/后端):
//! 1. 前端: [`TypedGraph::build`] — id interning + 双角色节点归位 +
//!    端口域解析 + 边分类 ([`node_hir::EdgeClass`]), 跨域边报 DomainMismatch
//! 2. 中端: 平面投影 ([`node_plane`]) — 值平面拓扑序 (f32 ∪ 字符串边,
//!    保证上游 string 节点先于下游 Str 节点求值) + 字节平面 [`BytePlan`];
//!    跨平面不构成循环由 EdgeFiltered 视图结构性保证
//! 3. 后端: [`node_lower::lower_value_plane`] — 槽位分配 (f32/字符串双 arena)
//!    + 平坦 `SlotPlan` / [`CompiledOp`] 序列 (热路径逐帧零字符串哈希)

use std::collections::HashMap;

use buffer_graph::Edge;
use petgraph::stable_graph::NodeIndex;
use petgraph::visit::{EdgeRef, IntoEdgeReferences};

use dsp_fft::SpectrumOutput;
use dsp_window::WindowType;
use node_kind::{DecoderBlockDef, NodeDef, NodeKind};

use node_eval::CompiledEval;
use node_hir::{CompileError, EdgeClass, HirEdge, TypedGraph};
use node_lower::lower_value_plane;
use node_plane::{value_plane, BytePlan};

/// 编译后的图 — 包含拓扑序的评估计划
pub struct CompiledGraph {
    pub tab_id: String,
    /// HIR — 类型化图 (节点双角色定义 + 分类边)
    hir: TypedGraph,
    /// 拓扑序 — 仅包含有 f32/String 输出的节点
    /// (ProtocolSource/Input/Math/Custom/Filter/FrameDecoder/Ifft/Str/Trigger/TextInput)
    /// Sink/SpectrumSink/Transport/Protocol 不参与值平面评估
    pub(crate) eval_order: Vec<String>,
    /// 反向索引: target_node → (target_handle → (source_node, source_handle))
    /// 嵌套结构支持 &str 零分配查询 (evaluate_into 热路径)
    pub(crate) input_index: HashMap<String, HashMap<String, (String, String)>>,
    /// 字符串输入反向索引 (结构同 input_index, 来自字符串边)
    pub(crate) string_input_index: HashMap<String, HashMap<String, (String, String)>>,
    /// 编译期缓存: Math 输入端口名 in0..inN (避免每帧 format! 分配)
    pub(crate) in_names: Vec<String>,
    /// 编译期槽位评估表 (逐帧评估零字符串哈希, process_frames_batch 热路径用)
    pub(crate) compiled: CompiledEval,
    /// 字节平面处理计划 (拓扑序 + 源→下游路由)
    pub(crate) byte_plan: BytePlan,
    /// 预计算节点 id 表 (访问器零分配; 各类节点在编译期一次收集)
    custom_nodes: Vec<String>,
    spectrum_sinks: Vec<String>,
    filters: Vec<String>,
    iffts: Vec<String>,
    decoders: Vec<String>,
}

impl CompiledGraph {
    /// 编译图 — HIR 构建 → 平面投影 → lowering, 检测循环/域不匹配
    pub fn compile(
        tab_id: String,
        nodes: Vec<NodeDef>,
        edges: Vec<Edge>,
    ) -> Result<Self, CompileError> {
        // 1. 前端: HIR (含端口域分类; 跨域边 → DomainMismatch)
        let hir = TypedGraph::build(nodes, edges)?;
        // 2. 中端: 值平面投影 (拓扑序 + 输入索引) + 字节平面计划
        let mir = value_plane(&hir)?;
        let byte_plan = BytePlan::build(&hir)?;
        // 3. 后端: 槽位 lowering
        let compiled = CompiledEval::new(lower_value_plane(&hir, &mir));

        let eval_order = mir
            .order
            .iter()
            .map(|&ix| hir.id_of(ix).to_string())
            .collect();

        // 预计算节点 id 表 (访问器返回切片, 零逐次分配)
        let mut custom_nodes = Vec::new();
        let mut spectrum_sinks = Vec::new();
        let mut filters = Vec::new();
        let mut iffts = Vec::new();
        let mut decoders = Vec::new();
        for n in hir.value_nodes() {
            match &n.kind {
                NodeKind::Custom { .. } => custom_nodes.push(n.id.clone()),
                NodeKind::SpectrumSink { .. } => spectrum_sinks.push(n.id.clone()),
                NodeKind::Filter { .. } => filters.push(n.id.clone()),
                NodeKind::Ifft => iffts.push(n.id.clone()),
                NodeKind::FrameDecoder { .. } => decoders.push(n.id.clone()),
                _ => {}
            }
        }

        Ok(Self {
            tab_id,
            hir,
            eval_order,
            input_index: mir.input_index,
            string_input_index: mir.string_input_index,
            in_names: mir.in_names,
            compiled,
            byte_plan,
            custom_nodes,
            spectrum_sinks,
            filters,
            iffts,
            decoders,
        })
    }

    /// HIR 边权重 → 原始 Edge (端点 id 从图端点取)
    fn to_edge(&self, er: impl EdgeRef<NodeId = NodeIndex, Weight = HirEdge>) -> Edge {
        Edge {
            id: er.weight().id.clone(),
            source: self.hir.id_of(er.source()).to_string(),
            source_handle: er.weight().source_handle.clone(),
            target: self.hir.id_of(er.target()).to_string(),
            target_handle: er.weight().target_handle.clone(),
        }
    }

    /// 全部边 (含字节边; 按插入序)
    pub fn edges(&self) -> impl Iterator<Item = Edge> + '_ {
        self.hir.graph.edge_references().map(|er| self.to_edge(er))
    }

    /// 字节路由边 (字节平面: 两端端口域均为 Bytes, 含 RawData 字节标记边)
    pub fn byte_edges(&self) -> impl Iterator<Item = Edge> + '_ {
        self.hir
            .graph
            .edge_references()
            .filter(|er| er.weight().class.in_byte_plane())
            .map(|er| self.to_edge(er))
    }

    /// 字符串路由边 (两端端口域均为 String) — 参与值平面拓扑排序
    pub fn string_edges(&self) -> impl Iterator<Item = Edge> + '_ {
        self.hir
            .graph
            .edge_references()
            .filter(|er| er.weight().class == EdgeClass::Str)
            .map(|er| self.to_edge(er))
    }

    /// 数值平面节点表迭代 (ProtocolSource/Input/Math/Sink 等;
    /// 不含 Transport/Protocol 字节平面定义 — 同一 id 可能同时是本 tab 的
    /// ProtocolSource 与全局 Protocol, 后者只参与字节平面)
    pub fn value_nodes(&self) -> impl Iterator<Item = &NodeDef> {
        self.hir.value_nodes()
    }

    /// 节点 id → 数值平面定义
    pub fn value_def(&self, id: &str) -> Option<&NodeDef> {
        self.hir.value_def(id)
    }

    /// 字节平面处理计划 (拓扑序 + 源→下游路由, 取代旧 loopback_targets_for)
    pub const fn byte_plan(&self) -> &BytePlan {
        &self.byte_plan
    }

    /// 编译期槽位评估表 (process_frames_batch 热路径用)
    pub const fn compiled(&self) -> &CompiledEval {
        &self.compiled
    }

    /// HIR 视图 (只读访问编译前端产物, 供 IPC `get_graph_hir` 序列化)
    pub const fn hir(&self) -> &TypedGraph {
        &self.hir
    }
}

// ============ 节点查询访问器 (编译期预计算, 返回切片零分配) ============

impl CompiledGraph {
    /// 获取所有 Custom 节点 id
    pub fn custom_node_ids(&self) -> &[String] {
        &self.custom_nodes
    }

    /// 获取所有 SpectrumSink 节点 id
    pub fn spectrum_sink_ids(&self) -> &[String] {
        &self.spectrum_sinks
    }

    /// 获取所有 Filter 节点 id (供状态清理: 删除节点时移除对应 filter_states)
    pub fn filter_node_ids(&self) -> &[String] {
        &self.filters
    }

    /// 获取所有 Ifft 节点 id (供状态清理 + spectrum_ticker 合成时域缓冲)
    pub fn ifft_node_ids(&self) -> &[String] {
        &self.iffts
    }

    /// 获取所有 FrameDecoder 节点 id
    /// (供 data_loop 同步 decoder_states: 创建/重建/清理 FrameParser)
    pub fn decoder_node_ids(&self) -> &[String] {
        &self.decoders
    }

    /// 解析 Ifft 节点的上游 FFT (SpectrumSink) 节点 id
    ///
    /// 输入端口固定为 "spectrum" (频域), 编译期从 input_index 反查边:
    /// (source 节点的 "spectrum" 输出) → source 节点 id。
    /// 无上游边返回 None。
    pub fn ifft_source(&self, node_id: &str) -> Option<String> {
        self.input_index
            .get(node_id)
            .and_then(|ports| ports.get("spectrum"))
            .map(|(src, _)| src.clone())
    }

    /// 获取 FrameDecoder 节点的配置 (blocks + 附加端口开关 + loopback 标志)
    /// 用于 decoder_feed 在节点变更时重建 FrameParser
    ///
    /// 注意: 返回的 loopback 标志为 deprecated (见 NodeKind::FrameDecoder),
    /// 新语义下字节来源完全由输入字节边决定 (见 byte_plan)。
    #[allow(clippy::type_complexity)]
    pub fn decoder_config(
        &self,
        node_id: &str,
    ) -> Option<(&[DecoderBlockDef], bool, bool, bool, bool, bool)> {
        let node = self.value_def(node_id)?;
        if let NodeKind::FrameDecoder {
            blocks,
            enable_valid,
            enable_frame_count,
            enable_last_timestamp,
            enable_fps,
            loopback,
        } = &node.kind
        {
            Some((
                blocks.as_slice(),
                *enable_valid,
                *enable_frame_count,
                *enable_last_timestamp,
                *enable_fps,
                *loopback,
            ))
        } else {
            None
        }
    }

    /// 获取 SpectrumSink 节点的配置 (window_size, window_type, output, sample_rate)
    /// 用于 state.rs 在节点变更时重建 SpectrumAnalyzer
    pub fn spectrum_sink_config(
        &self,
        node_id: &str,
    ) -> Option<(usize, WindowType, SpectrumOutput, f32)> {
        let node = self.value_def(node_id)?;
        if let NodeKind::SpectrumSink {
            window_size,
            window_type,
            output,
            sample_rate,
        } = &node.kind
        {
            Some((*window_size, *window_type, *output, *sample_rate))
        } else {
            None
        }
    }
}

// 测试模块已迁移至 src/compile_tests.rs (顶层 #[cfg(test)])
