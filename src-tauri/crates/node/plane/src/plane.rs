//! 平面投影 (中端 MIR) — 值平面/字节平面子图 + petgraph 拓扑排序
//!
//! 跨平面不构成循环的不变量由投影结构性保证: 各平面子图只含本平面边
//! ([`EdgeClass::in_byte_plane`] / [`EdgeClass::in_value_plane`]) 与本平面节点,
//!
//! 注: 投影构建为显式子图而非 `EdgeFiltered`/`NodeFiltered` 零拷贝视图 —
//! petgraph 0.8 的 NodeFiltered 未实现 IntoNeighborsDirected/IntoNodeIdentifiers,
//! 无法承载有向拓扑排序; 子图仅在编译期构建一次, 代价可忽略。

use std::collections::HashMap;
use std::hash::Hash;

use petgraph::algo::toposort;
use petgraph::stable_graph::{NodeIndex, StableDiGraph};
use petgraph::visit::{EdgeRef, IntoEdgeReferences, IntoNeighborsDirected};
use petgraph::Direction;
use rustc_hash::{FxHashMap, FxHashSet};

use kind::NodeKind;

use hir::CompileError;
use hir::{EdgeClass, HirNode, TypedGraph};

/// 值平面 MIR — 拓扑序 + 输入反查索引 + 编译期端口名缓存
#[derive(Debug)]
pub struct ValueMir {
    /// 值平面拓扑序 (仅含有值平面输出的节点, 见 [`has_value_output`])
    pub order: Vec<NodeIndex>,
    /// 反向索引: target_node → (target_handle → (source_node, source_handle))
    /// 嵌套结构支持 &str 零分配查询 (evaluate_into 热路径); 来自 f32 输入边
    pub input_index: HashMap<String, HashMap<String, (String, String)>>,
    /// 字符串输入反向索引 (结构同 input_index, 来自字符串边)
    pub string_input_index: HashMap<String, HashMap<String, (String, String)>>,
    /// 编译期缓存: Math 输入端口名 in0..inN (避免每帧 format! 分配)
    pub in_names: Vec<String>,
}

/// 节点是否有值平面输出 — 无输出者不进 eval_order:
/// - Sink: 纯消费, 无输出
/// - SpectrumSink: 块运算, 无输出端口, 由独立 30 FPS ticker 触发 FFT
/// - Transport/Protocol: 字节平面节点 (其定义只落在 byte_def, 此处天然不含)
/// - 占位节点 (边端点缺失) 无定义, 不参与求值
///
/// TextOut 参与求值序 (其 "text" 槽位透传 op 依赖拓扑序先上游后本节点),
/// 但它没有输出端口 — 下游不可能引用, 进序仅保证自身求值, 与消费端语义一致。
fn has_value_output(w: &HirNode) -> bool {
    w.value_def
        .as_ref()
        .is_some_and(|d| !matches!(d.kind, NodeKind::Sink | NodeKind::SpectrumSink { .. }))
}

/// 平面投影 — 按节点/边谓词抽取子图 (节点权重 = 原图 NodeIndex, 供结果映射回原图)
fn project(
    g: &TypedGraph,
    node_pred: impl Fn(&TypedGraph, NodeIndex) -> bool,
    edge_pred: impl Fn(EdgeClass) -> bool,
) -> StableDiGraph<NodeIndex, ()> {
    let mut sub = StableDiGraph::default();
    let mut map: FxHashMap<NodeIndex, NodeIndex> = FxHashMap::default();
    let mut intern = |ix: NodeIndex, sub: &mut StableDiGraph<NodeIndex, ()>| {
        *map.entry(ix).or_insert_with(|| sub.add_node(ix))
    };
    for er in g.graph.edge_references() {
        if !edge_pred(er.weight().class) {
            continue;
        }
        let (s, t) = (er.source(), er.target());
        if !node_pred(g, s) || !node_pred(g, t) {
            continue;
        }
        let s2 = intern(s, &mut sub);
        let t2 = intern(t, &mut sub);
        sub.add_edge(s2, t2, ());
    }
    // 无边孤立成员 (如孤立 Transport 仍是字节平面 dispatch 起点)
    for ix in g.graph.node_indices() {
        if node_pred(g, ix) {
            intern(ix, &mut sub);
        }
    }
    sub
}

/// 子图拓扑排序 + 环诊断, 结果经子图节点权重映射回原图 NodeIndex
fn topo(
    g: &TypedGraph,
    sub: &StableDiGraph<NodeIndex, ()>,
    err: impl FnOnce(Vec<String>) -> CompileError,
) -> Result<Vec<NodeIndex>, CompileError> {
    match toposort(sub, None) {
        Ok(order) => Ok(order.iter().map(|&ix| sub[ix]).collect()),
        Err(c) => {
            let cycle = extract_cycle(sub, c.node_id())
                .iter()
                .map(|&ix| g.id_of(sub[ix]).to_string())
                .collect();
            Err(err(cycle))
        }
    }
}

/// 值平面投影: 编译拓扑序 + 输入索引
///
/// 子图 = 值平面边 (f32 ∪ 字符串 ∪ RawData 数值标记) × 有值平面输出的节点;
/// 平面内循环 → [`CompileError::Cycle`] (完整环路径)。
pub fn value_plane(g: &TypedGraph) -> Result<ValueMir, CompileError> {
    let sub = project(
        g,
        |g, ix| has_value_output(&g.graph[ix]),
        EdgeClass::in_value_plane,
    );
    let order = topo(g, &sub, |cycle| CompileError::Cycle { cycle })?;

    // 输入反查索引: 按边插入序遍历 (后者覆盖前者);
    // 占位端点同样入索引 — 反查时上游无槽位/无输出, 按缺省 0.0 处理
    let mut input_index: HashMap<String, HashMap<String, (String, String)>> = HashMap::new();
    let mut string_input_index: HashMap<String, HashMap<String, (String, String)>> = HashMap::new();
    for er in g.graph.edge_references() {
        let class = er.weight().class;
        let entry = (
            g.id_of(er.source()).to_string(),
            er.weight().source_handle.clone(),
        );
        if class.is_f32_input() {
            input_index
                .entry(g.id_of(er.target()).to_string())
                .or_default()
                .insert(er.weight().target_handle.clone(), entry);
        } else if class == EdgeClass::Str {
            string_input_index
                .entry(g.id_of(er.target()).to_string())
                .or_default()
                .insert(er.weight().target_handle.clone(), entry);
        }
    }

    // 编译期端口名缓存 (evaluate 热路径避免 format! 分配)
    let max_inputs = g
        .value_nodes()
        .map(|n| match &n.kind {
            NodeKind::Math { input_count, .. } => *input_count,
            _ => 0,
        })
        .max()
        .unwrap_or(0);
    let in_names: Vec<String> = (0..max_inputs).map(|i| format!("in{i}")).collect();

    Ok(ValueMir {
        order,
        input_index,
        string_input_index,
        in_names,
    })
}

/// 字节平面节点集: 字节平面定义 (Transport/Protocol) ∪ 字节边端点 (有定义的;
/// 占位端点不参与平面拓扑, 但仍出现在 consumers 路由表)
fn byte_plane_nodes(g: &TypedGraph) -> FxHashSet<NodeIndex> {
    let graph = &g.graph;
    let mut set: FxHashSet<NodeIndex> = graph
        .node_indices()
        .filter(|&ix| graph[ix].byte_def.is_some())
        .collect();
    for er in graph.edge_references() {
        if er.weight().class.in_byte_plane() {
            for ix in [er.source(), er.target()] {
                let w = &graph[ix];
                if w.value_def.is_some() || w.byte_def.is_some() {
                    set.insert(ix);
                }
            }
        }
    }
    set
}

/// 字节平面拓扑序 — 平面内循环 → [`CompileError::ByteCycle`] (完整环路径)
pub fn byte_plane_order(g: &TypedGraph) -> Result<Vec<NodeIndex>, CompileError> {
    let nodes = byte_plane_nodes(g);
    let sub = project(g, |_, ix| nodes.contains(&ix), EdgeClass::in_byte_plane);
    topo(g, &sub, |cycle| CompileError::ByteCycle { cycle })
}

/// 三色 DFS 提取完整环路径 (首节点在尾部重复出现, 如 a → b → a)
///
/// `start` 为 toposort 报告的在环节点; 沿出边找栈内回边, 截取栈上环段。
fn extract_cycle<G>(g: G, start: G::NodeId) -> Vec<G::NodeId>
where
    G: IntoNeighborsDirected + Copy,
    G::NodeId: Copy + Eq + Hash,
{
    fn dfs<G>(
        g: G,
        u: G::NodeId,
        color: &mut FxHashMap<G::NodeId, u8>,
        stack: &mut Vec<G::NodeId>,
    ) -> Option<Vec<G::NodeId>>
    where
        G: IntoNeighborsDirected + Copy,
        G::NodeId: Copy + Eq + Hash,
    {
        color.insert(u, 1);
        stack.push(u);
        for v in g.neighbors_directed(u, Direction::Outgoing) {
            match color.get(&v).copied() {
                Some(1) => {
                    let pos = stack
                        .iter()
                        .position(|&x| x == v)
                        .expect("访问中节点必在栈中");
                    let mut cycle = stack[pos..].to_vec();
                    cycle.push(v);
                    return Some(cycle);
                }
                Some(2) => {}
                _ => {
                    if let Some(c) = dfs(g, v, color, stack) {
                        return Some(c);
                    }
                }
            }
        }
        stack.pop();
        color.insert(u, 2);
        None
    }

    let mut color = FxHashMap::default();
    let mut stack = Vec::new();
    dfs(g, start, &mut color, &mut stack).unwrap_or_else(|| vec![start])
}

#[cfg(test)]
mod tests {
    use super::*;
    use kind::MathOp;
    use testkit::*;

    fn ids<'a>(g: &'a TypedGraph, order: &[NodeIndex]) -> Vec<&'a str> {
        order.iter().map(|&ix| g.id_of(ix)).collect()
    }

    #[test]
    fn test_value_plane_topo_order() {
        // ps1 → m1, ps1 → s1(Str); Sink/Transport 不进值平面
        let g = TypedGraph::build(
            vec![
                make_protocol_source("ps1", "t1", "proto1", 1),
                make_math("m1", "t1", MathOp::Add, 1),
                make_sink("sink1", "t1"),
                make_transport("tp"),
            ],
            vec![
                edge("e1", "ps1", "ch0", "m1", "in0"),
                edge("e2", "m1", "result", "sink1", "value"),
            ],
        )
        .unwrap();
        let mir = value_plane(&g).unwrap();
        let order = ids(&g, &mir.order);
        let pos = |id: &str| order.iter().position(|&n| n == id).unwrap();
        assert!(pos("ps1") < pos("m1"));
        assert!(!order.contains(&"sink1"));
        assert!(!order.contains(&"tp"));
        // 输入索引
        let (src, port) = &mir.input_index["m1"]["in0"];
        assert_eq!((src.as_str(), port.as_str()), ("ps1", "ch0"));
    }

    #[test]
    fn test_value_plane_cycle_full_path() {
        let g = TypedGraph::build(
            vec![
                make_math("a", "t1", MathOp::Add, 1),
                make_math("b", "t1", MathOp::Add, 1),
            ],
            vec![
                edge("e1", "a", "result", "b", "in0"),
                edge("e2", "b", "result", "a", "in0"),
            ],
        )
        .unwrap();
        match value_plane(&g) {
            Err(CompileError::Cycle { cycle }) => {
                // 完整环路径: 首尾同节点, 两个节点都在环上
                assert_eq!(cycle.first(), cycle.last());
                assert!(cycle.contains(&"a".to_string()));
                assert!(cycle.contains(&"b".to_string()));
            }
            other => panic!("应报数值平面循环, 实际: {other:?}"),
        }
    }

    #[test]
    fn test_cross_plane_no_false_cycle() {
        // FrameDecoder 输出 (F32) → Command 输入; Command.loopbackOut (Bytes)
        // → FrameDecoder.loopbackIn (Bytes): 跨平面不构成循环
        let g = TypedGraph::build(
            vec![make_decoder("dec", "t1"), make_sink("cmd", "t1")],
            vec![
                edge("e1", "dec", "value", "cmd", "value"),
                edge("e2", "cmd", "loopbackOut", "dec", "loopbackIn"),
            ],
        )
        .unwrap();
        assert!(value_plane(&g).is_ok());
        assert!(byte_plane_order(&g).is_ok());
    }

    #[test]
    fn test_value_plane_excludes_ghost_endpoints() {
        // 占位端点 (无定义) 不参与值平面拓扑, 但边仍进 input_index
        // (反查时上游无槽位 → 缺省 0.0)
        let g = TypedGraph::build(
            vec![make_math("m1", "t1", MathOp::Add, 1)],
            vec![edge("e1", "ghost", "result", "m1", "in0")],
        )
        .unwrap();
        let mir = value_plane(&g).unwrap();
        let order = ids(&g, &mir.order);
        assert!(order.contains(&"m1"));
        assert!(!order.contains(&"ghost"));
        assert!(mir.input_index["m1"].contains_key("in0"));
    }
}
