//! 图触发/历史记录谓词 — 热路径与并发路径共用

use buffer_graph::Edge;
use engine::CompiledGraph;
use kind::NodeKind;

/// 图是否被指定源触发:
/// - 引用了该 Protocol 源 (ProtocolSource.node_id == source_id) → 触发
/// - 不含任何 ProtocolSource (Input/Math/Custom 等纯本地图) → 任意源来帧都触发
///   (沿用旧单源架构行为: 所有图每帧评估)
/// - 引用了其他源 → 不触发 (该源来帧时才评估)
pub fn graph_triggered_by(g: &CompiledGraph, source_id: &str) -> bool {
    let mut has_source = false;
    for n in g.value_nodes() {
        if let NodeKind::ProtocolSource { node_id, .. } = &n.kind {
            has_source = true;
            if node_id == source_id {
                return true;
            }
        }
    }
    !has_source
}

/// 只有 Waveform 的 `CH<n>` 输入边需要持久化派生历史。
///
/// 旧路径把图内每条边（包括 Math 的中间输入、Gauge/Label 等只读快照的边）
/// 都复制进派生环，令一次求值产生数倍无消费者写放大。Waveform 端口契约由
/// 前端 `WidgetPorts` 固定为大写 `CH0..CHn`，这里严格校验整个后缀为数字。
fn numbered_handle<'a>(handle: &'a str, prefix: &str) -> Option<&'a str> {
    handle
        .strip_prefix(prefix)
        .filter(|suffix| !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()))
}

pub fn records_waveform_history(graph: &CompiledGraph, edge: &Edge) -> bool {
    if numbered_handle(&edge.target_handle, "CH").is_none() {
        return false;
    }
    // JustFloat/FireWater 的标准 ch<n> 已由原始 DataBuffer 保存，前端也直接从
    // channels[n] 读取；再写一份派生环既无消费者又把内存/CPU 放大一倍。
    !(numbered_handle(&edge.source_handle, "ch").is_some()
        && matches!(
            graph.value_def(&edge.source).map(|node| &node.kind),
            Some(NodeKind::ProtocolSource { .. })
        ))
}

/// 指定来源是否存在必须逐帧执行的数值图。
///
/// 仅 `ProtocolSource → Sink` 的图只需要批尾最新值；原始 ch<n> 波形历史已经
/// 在记录平面完整入库。将这类常见纯波形/仪表图压缩为每批一次求值，避免为了
/// 刷新 latest-value 快照重复遍历几十万帧。
pub fn graph_requires_full_batch(graph: &CompiledGraph) -> bool {
    graph
        .value_nodes()
        .any(|node| !matches!(node.kind, NodeKind::ProtocolSource { .. } | NodeKind::Sink))
        || graph
            .edges()
            .any(|edge| records_waveform_history(graph, &edge))
}
