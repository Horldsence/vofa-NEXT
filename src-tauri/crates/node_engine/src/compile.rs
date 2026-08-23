//! 图编译 — CompiledGraph::compile + CompileError + 编译期槽位表构建
//!
//! 编译流程:
//! 1. 边按两端端口域分类: 均 Bytes → byte_edges; 均 F32 → f32_edges;
//!    均 String → string_edges; 不匹配 → [`CompileError::DomainMismatch`]
//!    (取代旧 LOOPBACK_IN_HANDLE 字符串特判);
//!    例外: RawData 关联通道边 (Sink 的 src: 动态端口) 按源端域归类放行, 见
//!    [`is_raw_data_channel_target`]
//! 2. 字节平面: [`BytePlan::build`] 独立拓扑排序 (字节平面内循环 → ByteCycle;
//!    跨平面不构成循环 — 值平面 DFS 只看 f32_edges + string_edges)
//! 3. 值平面: 对 f32_edges + string_edges 三色 DFS 拓扑排序 (保证上游 string
//!    节点先于下游 Str 节点求值; 字符串槽位分配独立于 f32 槽位),
//!    构建 input_index + string_input_index + 槽位评估表

use std::collections::HashMap;

use buffer_graph::Edge;
use rustc_hash::FxBuildHasher;

use node_kind::{port_domain, NodeDef, NodeKind, PortDomain, RAW_DATA_PORT_PREFIX};

use crate::byte_plan::BytePlan;
use crate::eval::{str_num_default, CompiledEval, CompiledOp};

/// 编译后的图 — 包含拓扑序的评估计划
pub struct CompiledGraph {
    pub tab_id: String,
    /// 数值平面节点表 (ProtocolSource/Input/Math/Sink 等; 不含 Transport/Protocol
    /// 字节平面定义 — 同一 id 可能同时是本 tab 的 ProtocolSource 与全局 Protocol,
    /// 后者只参与字节边分类与 BytePlan 编译)
    pub(crate) nodes: HashMap<String, NodeDef>,
    /// 边集合 (全部边, 含字节边)
    pub(crate) edges: Vec<Edge>,
    /// 字节路由边 (两端端口域均为 Bytes) — 不参与值平面拓扑排序/求值
    /// (字节不经 evaluate 流动; 若参与 DFS, Command var_ref 输入回连解码器输出会误判循环)
    pub(crate) byte_edges: Vec<Edge>,
    /// 字符串路由边 (两端端口域均为 String) — 参与值平面拓扑排序, 供慢路径解析字符串输入
    pub(crate) string_edges: Vec<Edge>,
    /// 拓扑序 — 仅包含有 f32/String 输出的节点
    /// (ProtocolSource/Input/Math/Custom/Filter/FrameDecoder/Ifft/Str/Trigger/TextInput)
    /// Sink/SpectrumSink/Transport/Protocol 不参与值平面评估
    pub(crate) eval_order: Vec<String>,
    /// 反向索引: target_node → (target_handle → (source_node, source_handle))
    /// 嵌套结构支持 &str 零分配查询 (evaluate_into 热路径)
    pub(crate) input_index: HashMap<String, HashMap<String, (String, String)>>,
    /// 字符串输入反向索引 (结构同 input_index, 来自 string_edges)
    pub(crate) string_input_index: HashMap<String, HashMap<String, (String, String)>>,
    /// 编译期缓存: Math 输入端口名 in0..inN (避免每帧 format! 分配)
    pub(crate) in_names: Vec<String>,
    /// 编译期槽位评估表 (逐帧评估零字符串哈希, process_frames_batch 热路径用)
    pub(crate) compiled: CompiledEval,
    /// 字节平面处理计划 (拓扑序 + 源→下游路由)
    pub(crate) byte_plan: BytePlan,
}

/// 图编译错误 — 强类型变体,无 `String` catch-all。
#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    #[error("节点 {id} 不存在于图中")]
    NodeNotFound { id: String },

    #[error("数值平面检测到循环连接: {cycle:?}")]
    Cycle { cycle: Vec<String> },

    #[error("字节平面检测到循环连接: {cycle:?}")]
    ByteCycle { cycle: Vec<String> },

    #[error("边 {edge_id} 端口域不匹配: {source_node}.{source_port:?} ({src_domain:?}) → {target}.{target_port:?} ({tgt_domain:?})")]
    DomainMismatch {
        edge_id: String,
        source_node: String,
        source_port: String,
        src_domain: PortDomain,
        target: String,
        target_port: String,
        tgt_domain: PortDomain,
    },
}

impl error::Error for CompileError {
    fn kind(&self) -> &'static str {
        "Graph"
    }
}

impl From<CompileError> for error::AppError {
    fn from(e: CompileError) -> Self {
        Self::Graph(Box::new(e))
    }
}

/// 判定边目标是否为 RawData 控件的关联通道端口 (Sink + `src:` 动态端口 id)
/// RawData 是唯一使用 src: 端口约定的节点 (编译为 NodeKind::Sink);
/// 其他 Sink (Gauge/Command 等) 的端口不带此前缀, 跨域校验不受影响
fn is_raw_data_channel_target(
    node_map: &HashMap<String, NodeDef>,
    target: &str,
    target_handle: &str,
) -> bool {
    target_handle.starts_with(RAW_DATA_PORT_PREFIX)
        && matches!(node_map.get(target).map(|n| &n.kind), Some(NodeKind::Sink))
}

impl CompiledGraph {
    /// 编译图 — 构建拓扑序 + 索引, 检测循环
    pub fn compile(
        tab_id: String,
        nodes: Vec<NodeDef>,
        edges: Vec<Edge>,
    ) -> Result<Self, CompileError> {
        fn dfs(
            id: &str,
            nodes: &HashMap<String, NodeDef>,
            edges: &[Edge],
            visited: &mut HashMap<String, u8>,
            order: &mut Vec<String>,
            cycle: &mut Vec<String>,
        ) -> Result<(), CompileError> {
            match visited.get(id) {
                Some(&1) => {
                    cycle.push(id.to_string());
                    return Err(CompileError::Cycle {
                        cycle: cycle.clone(),
                    });
                }
                Some(&2) => return Ok(()),
                _ => {}
            }
            visited.insert(id.to_string(), 1);

            // 访问上游 (有 edge 指向本节点的源节点)
            for e in edges {
                if e.target == id && nodes.contains_key(&e.source) {
                    dfs(&e.source, nodes, edges, visited, order, cycle)?;
                }
            }

            visited.insert(id.to_string(), 2);
            order.push(id.to_string());
            Ok(())
        }

        // 节点表按平面分离: 同一 id 可能同时存在全局 Protocol 定义 (字节平面)
        // 与本 tab 的 ProtocolSource 引用 (数值平面) — 后者携带 ch0..chN 槽位,
        // 若被前者覆盖会导致通道输出恒为 0。
        let mut node_map: HashMap<String, NodeDef> = HashMap::new();
        let mut byte_nodes: HashMap<String, NodeDef> = HashMap::new();
        for n in nodes {
            if matches!(
                n.kind,
                NodeKind::Transport { .. } | NodeKind::Protocol { .. }
            ) {
                byte_nodes.insert(n.id.clone(), n);
            } else {
                node_map.insert(n.id.clone(), n);
            }
        }
        // 全量视图 (字节平面定义优先) — 仅供 BytePlan 编译
        let mut all_nodes = node_map.clone();
        for (id, def) in &byte_nodes {
            all_nodes.insert(id.clone(), def.clone());
        }

        // 端口域查询: 字节平面定义只在判定为 Bytes 时生效 (handle 命名两平面正交),
        // 否则回落数值平面定义; 端点节点缺失按 F32 处理 (与旧版容错语义一致)
        let domain_of = |id: &str, handle: &str, is_output: bool| -> PortDomain {
            if let Some(bn) = byte_nodes.get(id) {
                let d = port_domain(&bn.kind, handle, is_output);
                if d == PortDomain::Bytes {
                    return d;
                }
            }
            node_map
                .get(id)
                .map_or(PortDomain::F32, |n| port_domain(&n.kind, handle, is_output))
        };

        // 边按两端端口域分类
        let mut byte_edges: Vec<Edge> = Vec::new();
        let mut f32_edges: Vec<Edge> = Vec::new();
        let mut string_edges: Vec<Edge> = Vec::new();
        for e in &edges {
            let src_domain = domain_of(&e.source, &e.source_handle, true);
            let tgt_domain = domain_of(&e.target, &e.target_handle, false);
            match (src_domain, tgt_domain) {
                (PortDomain::Bytes, PortDomain::Bytes) => byte_edges.push(e.clone()),
                (PortDomain::F32, PortDomain::F32) => f32_edges.push(e.clone()),
                (PortDomain::String, PortDomain::String) => string_edges.push(e.clone()),
                // RawData 关联通道边 (Sink 的 src:<source>:<handle> 动态端口):
                // 边只是用户意图标记, 字节/数值都不经 evaluate 流入 — 按源端域归类放行,
                // 字节边进 BytePlan 后由字节路由的默认分支忽略 (RawData 视图走订阅旁路)
                _ if is_raw_data_channel_target(&node_map, &e.target, &e.target_handle) => {
                    match src_domain {
                        PortDomain::Bytes => byte_edges.push(e.clone()),
                        PortDomain::F32 => f32_edges.push(e.clone()),
                        PortDomain::String => {} // 字符串平面不进 f32/byte 边
                    }
                }
                // 其余组合 (String↔F32 / String↔Bytes 等跨域) 一律域不匹配
                _ => {
                    return Err(CompileError::DomainMismatch {
                        edge_id: e.id.clone(),
                        source_node: e.source.clone(),
                        source_port: e.source_handle.clone(),
                        src_domain,
                        target: e.target.clone(),
                        target_port: e.target_handle.clone(),
                        tgt_domain,
                    });
                }
            }
        }

        // 字节平面拓扑 (独立; 跨平面边不构成循环)
        // 用全量视图 (含 Transport/Protocol 定义) 编译
        let byte_plan = BytePlan::build(&all_nodes, &byte_edges)?;

        // 构建 input_index: target → (target_handle → (source, source_handle))
        // 嵌套结构, 支持 &str 零分配查询 (仅 f32 边参与求值)
        let mut input_index: HashMap<String, HashMap<String, (String, String)>> = HashMap::new();
        for e in &f32_edges {
            input_index.entry(e.target.clone()).or_default().insert(
                e.target_handle.clone(),
                (e.source.clone(), e.source_handle.clone()),
            );
        }

        // 字符串输入反向索引 (结构同 input_index, 来自 string_edges)
        let mut string_input_index: HashMap<String, HashMap<String, (String, String)>> =
            HashMap::new();
        for e in &string_edges {
            string_input_index
                .entry(e.target.clone())
                .or_default()
                .insert(
                    e.target_handle.clone(),
                    (e.source.clone(), e.source_handle.clone()),
                );
        }

        // 编译期端口名缓存 (evaluate 热路径避免 format! 分配)
        let max_inputs = node_map
            .values()
            .map(|n| match &n.kind {
                NodeKind::Math { input_count, .. } => *input_count,
                _ => 0,
            })
            .max()
            .unwrap_or(0);
        let in_names: Vec<String> = (0..max_inputs).map(|i| format!("in{i}")).collect();

        // 拓扑排序 — 仅对有值平面输出的节点
        // 使用 DFS 后序
        let mut visited: HashMap<String, u8> = HashMap::new(); // 0=未访问, 1=访问中, 2=已完成
        let mut order: Vec<String> = Vec::new();

        // 仅对有值平面输出的节点启动 DFS:
        // - Sink: 纯消费, 无输出
        // - SpectrumSink: 块运算, 无输出端口, 由独立 30 FPS ticker 触发 FFT
        // - Transport/Protocol: 字节平面节点, 无值平面输出
        let output_node_ids: Vec<String> = node_map
            .iter()
            .filter(|(_, n)| {
                !matches!(
                    n.kind,
                    NodeKind::Sink
                        | NodeKind::SpectrumSink { .. }
                        | NodeKind::Transport { .. }
                        | NodeKind::Protocol { .. }
                )
            })
            .map(|(id, _)| id.clone())
            .collect();

        // 值平面依赖边 = f32 边 + 字符串边: 字符串槽位分配独立于 f32 槽位,
        // 但拓扑序必须保证上游 string 节点先于下游 Str 节点求值 (慢/快路径同序)
        let mut dep_edges = f32_edges.clone();
        dep_edges.extend(string_edges.iter().cloned());

        let mut cycle: Vec<String> = Vec::new();
        for id in &output_node_ids {
            dfs(
                id,
                &node_map,
                &dep_edges,
                &mut visited,
                &mut order,
                &mut cycle,
            )?;
        }

        // 编译期槽位评估表 (材料齐备: eval_order/input_index/string_input_index/in_names)
        let compiled = CompiledEval::build(
            &node_map,
            &order,
            &input_index,
            &string_input_index,
            &in_names,
        );

        Ok(Self {
            tab_id,
            nodes: node_map,
            edges,
            byte_edges,
            string_edges,
            eval_order: order,
            input_index,
            string_input_index,
            in_names,
            compiled,
            byte_plan,
        })
    }

    pub const fn nodes(&self) -> &HashMap<String, NodeDef> {
        &self.nodes
    }

    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// 字节路由边 (两端端口域均为 Bytes 的边)
    pub fn byte_edges(&self) -> &[Edge] {
        &self.byte_edges
    }

    /// 字符串路由边 (两端端口域均为 String 的边)
    pub fn string_edges(&self) -> &[Edge] {
        &self.string_edges
    }

    /// 字节平面处理计划 (拓扑序 + 源→下游路由, 取代旧 loopback_targets_for)
    pub const fn byte_plan(&self) -> &BytePlan {
        &self.byte_plan
    }

    /// 编译期槽位评估表 (process_frames_batch 热路径用)
    pub const fn compiled(&self) -> &CompiledEval {
        &self.compiled
    }
}

// ============ 编译期槽位评估表构建 ============

/// 分配一个输出槽位 (同名端口重复分配时后者覆盖索引, 与 set_port 覆盖写语义一致)
fn alloc_slot(
    slot_names: &mut Vec<(String, String)>,
    slot_index: &mut HashMap<(String, String), usize, FxBuildHasher>,
    node_id: &str,
    port: &str,
) -> usize {
    let idx = slot_names.len();
    slot_names.push((node_id.to_string(), port.to_string()));
    slot_index.insert((node_id.to_string(), port.to_string()), idx);
    idx
}

/// 输入边 (node_id, in_name) → 上游输出槽位 (无边/无槽位 = None, 与 resolve_input 缺省 0.0 对应)
fn resolve_slot(
    input_index: &HashMap<String, HashMap<String, (String, String)>>,
    slot_index: &HashMap<(String, String), usize, FxBuildHasher>,
    node_id: &str,
    in_name: &str,
) -> Option<usize> {
    input_index
        .get(node_id)
        .and_then(|ports| ports.get(in_name))
        .and_then(|(sn, sp)| slot_index.get(&(sn.clone(), sp.clone())).copied())
}

impl CompiledEval {
    /// 编译期构建: 遍历 eval_order 按节点 kind 分配输出槽位 + 生成平坦操作序列
    ///
    /// 输入边在编译期经 input_index 反查 slot_index 解析为槽位下标;
    /// 查不到 = 常量 0.0 (与 resolve_input 缺省语义一致, 以 None 表示)。
    pub(crate) fn build(
        nodes: &HashMap<String, NodeDef>,
        eval_order: &[String],
        input_index: &HashMap<String, HashMap<String, (String, String)>>,
        string_input_index: &HashMap<String, HashMap<String, (String, String)>>,
        in_names: &[String],
    ) -> Self {
        let mut slot_names: Vec<(String, String)> = Vec::new();
        let mut slot_index: HashMap<(String, String), usize, FxBuildHasher> = HashMap::default();
        let mut str_slot_names: Vec<(String, String)> = Vec::new();
        let mut str_slot_index: HashMap<(String, String), usize, FxBuildHasher> =
            HashMap::default();
        let mut ops: Vec<CompiledOp> = Vec::new();
        // ProtocolSource 帧源表 (去重): node_id → frame_sources 下标
        let mut frame_sources: Vec<String> = Vec::new();

        for node_id in eval_order {
            let Some(node) = nodes.get(node_id) else {
                continue;
            };
            match &node.kind {
                NodeKind::ProtocolSource {
                    node_id: source_id,
                    channels,
                    port_names,
                } => {
                    let src = frame_sources
                        .iter()
                        .position(|s| s == source_id)
                        .unwrap_or_else(|| {
                            frame_sources.push(source_id.clone());
                            frame_sources.len() - 1
                        });
                    let names =
                        node_kind::protocol_source_port_names(port_names.as_deref(), *channels);
                    for (i, port) in names.iter().enumerate() {
                        if port == "str" {
                            // "str" 端口 (String 域, RawData 原始字节文本) → 字符串槽位,
                            // 不占数值槽位 (与 port_domain 的域划分一致)
                            let slot =
                                alloc_slot(&mut str_slot_names, &mut str_slot_index, node_id, port);
                            ops.push(CompiledOp::ProtocolSourceStr { src, slot });
                        } else {
                            let slot =
                                alloc_slot(&mut slot_names, &mut slot_index, node_id, port);
                            ops.push(CompiledOp::ProtocolSource { src, ch: i, slot });
                        }
                    }
                }
                NodeKind::Input => {
                    let slot = alloc_slot(&mut slot_names, &mut slot_index, node_id, "value");
                    ops.push(CompiledOp::Input {
                        node_id: node_id.clone(),
                        slot,
                    });
                }
                NodeKind::Math { op, input_count } => {
                    let inputs = (0..*input_count)
                        .map(|i| {
                            let in_name =
                                in_names.get(i).cloned().unwrap_or_else(|| format!("in{i}"));
                            resolve_slot(input_index, &slot_index, node_id, &in_name)
                        })
                        .collect();
                    let out = alloc_slot(&mut slot_names, &mut slot_index, node_id, "result");
                    ops.push(CompiledOp::Math {
                        op: *op,
                        inputs,
                        out,
                    });
                }
                NodeKind::Custom { outputs, .. } => {
                    let ports = outputs
                        .iter()
                        .map(|p| {
                            (
                                p.clone(),
                                alloc_slot(&mut slot_names, &mut slot_index, node_id, p),
                            )
                        })
                        .collect();
                    ops.push(CompiledOp::Custom {
                        node_id: node_id.clone(),
                        ports,
                    });
                }
                NodeKind::Filter { kind } => {
                    let input = resolve_slot(input_index, &slot_index, node_id, "in0");
                    let out = alloc_slot(&mut slot_names, &mut slot_index, node_id, "result");
                    ops.push(CompiledOp::Filter {
                        node_id: node_id.clone(),
                        kind: kind.clone(),
                        input,
                        out,
                    });
                }
                NodeKind::FrameDecoder {
                    blocks,
                    enable_valid,
                    enable_frame_count,
                    enable_last_timestamp,
                    enable_fps,
                    ..
                } => {
                    let mut ports = Vec::new();
                    for b in blocks {
                        if let Some(port) = b.output_port_name() {
                            let slot = alloc_slot(&mut slot_names, &mut slot_index, node_id, port);
                            ports.push((port.to_string(), slot));
                        }
                    }
                    let valid = enable_valid
                        .then(|| alloc_slot(&mut slot_names, &mut slot_index, node_id, "valid"));
                    let frame_count = enable_frame_count.then(|| {
                        alloc_slot(&mut slot_names, &mut slot_index, node_id, "frame_count")
                    });
                    let last_timestamp = enable_last_timestamp.then(|| {
                        alloc_slot(&mut slot_names, &mut slot_index, node_id, "last_timestamp")
                    });
                    let fps = enable_fps
                        .then(|| alloc_slot(&mut slot_names, &mut slot_index, node_id, "fps"));
                    ops.push(CompiledOp::FrameDecoder {
                        node_id: node_id.clone(),
                        ports,
                        valid,
                        frame_count,
                        last_timestamp,
                        fps,
                    });
                }
                NodeKind::Ifft => {
                    let out = alloc_slot(&mut slot_names, &mut slot_index, node_id, "out0");
                    ops.push(CompiledOp::Ifft {
                        node_id: node_id.clone(),
                        out,
                    });
                }
                NodeKind::Str { op, num } => {
                    // 输入按 StrOp::input_ports() 端口表顺序紧凑拆分为两个 Vec
                    // (只含同 domain 端口, 与 StrOp::evaluate 的 str_inputs/num_inputs
                    // 紧凑对齐约定一致; run 与 evaluate 均按此解析):
                    // - String 端口 → str_inputs: 经 string_input_index 反查上游
                    //   (node, port) 的字符串槽位; 查不到 (未连接) = None ↔ 缺省 ""
                    // - F32 端口 → num_inputs (无边 = None) + num_defaults
                    //   (编译期从 num 捕获的内联回退值, 与 num_inputs 等长)
                    let mut str_inputs = Vec::new();
                    let mut num_inputs = Vec::new();
                    let mut num_defaults = Vec::new();
                    for (name, domain) in op.input_ports() {
                        match domain {
                            PortDomain::String => str_inputs.push(resolve_slot(
                                string_input_index,
                                &str_slot_index,
                                node_id,
                                name,
                            )),
                            PortDomain::F32 => {
                                num_inputs.push(resolve_slot(
                                    input_index,
                                    &slot_index,
                                    node_id,
                                    name,
                                ));
                                num_defaults.push(str_num_default(num, name));
                            }
                            PortDomain::Bytes => {} // Str 端口表无 Bytes, 防御
                        }
                    }
                    // 输出端口固定 "result", 域由 op 决定
                    let (text_out, num_out) = match op.output_domain() {
                        PortDomain::String => (
                            Some(alloc_slot(
                                &mut str_slot_names,
                                &mut str_slot_index,
                                node_id,
                                "result",
                            )),
                            None,
                        ),
                        PortDomain::F32 => (
                            None,
                            Some(alloc_slot(
                                &mut slot_names,
                                &mut slot_index,
                                node_id,
                                "result",
                            )),
                        ),
                        PortDomain::Bytes => (None, None), // output_domain 无 Bytes, 防御
                    };
                    ops.push(CompiledOp::Str {
                        op: *op,
                        str_inputs,
                        num_inputs,
                        num_defaults,
                        text_out,
                        num_out,
                    });
                }
                NodeKind::Trigger {
                    mode,
                    edge,
                    default_miss,
                    default_miss_text,
                    command,
                    rules,
                } => {
                    // value/matched 分配 f32 槽位, text 分配字符串槽位
                    // (Trigger.text 由此可被 Str 字符串输入解析, 补上字符串平面缺口);
                    // auto 模式的 "trigger" 输入端口经 input_index 解析 (无边 = None → 0.0)
                    let trigger_in = resolve_slot(input_index, &slot_index, node_id, "trigger");
                    let value = alloc_slot(&mut slot_names, &mut slot_index, node_id, "value");
                    let matched = alloc_slot(&mut slot_names, &mut slot_index, node_id, "matched");
                    let text =
                        alloc_slot(&mut str_slot_names, &mut str_slot_index, node_id, "text");
                    ops.push(CompiledOp::Trigger {
                        node_id: node_id.clone(),
                        mode: mode.clone(),
                        edge: edge.clone(),
                        default_miss: *default_miss,
                        default_miss_text: default_miss_text.clone(),
                        command: command.clone(),
                        rules: rules.clone(),
                        trigger_in,
                        value,
                        matched,
                        text,
                    });
                }
                NodeKind::TextInput { text } => {
                    // 输出端口固定 "str" → 字符串槽位 (TextInput.str 可被 Str 字符串输入解析)
                    let out = alloc_slot(&mut str_slot_names, &mut str_slot_index, node_id, "str");
                    ops.push(CompiledOp::TextInput {
                        text: text.clone(),
                        out,
                    });
                }
                NodeKind::Sink
                | NodeKind::SpectrumSink { .. }
                | NodeKind::Transport { .. }
                | NodeKind::Protocol { .. } => {
                    // 无值平面输出的节点不应出现在 eval_order 中, 防御性跳过
                }
            }
        }

        // SpectrumSink 输入槽位 (不在 eval_order, 输入端口固定 "in0")
        let mut spectrum_slots = Vec::new();
        for (node_id, node) in nodes {
            if matches!(node.kind, NodeKind::SpectrumSink { .. }) {
                spectrum_slots.push((
                    node_id.clone(),
                    resolve_slot(input_index, &slot_index, node_id, "in0"),
                ));
            }
        }

        Self {
            slot_names,
            slot_index,
            ops,
            spectrum_slots,
            frame_sources,
            str_slot_names,
            str_slot_index,
        }
    }
}

// 测试模块已迁移至 src/compile_tests.rs / eval_tests.rs / equiv_tests.rs (顶层 #[cfg(test)])
