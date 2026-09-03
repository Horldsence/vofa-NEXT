//! 评估单元切分 — 供给/计算节点分类 + 计算节点连通分量 (并查集)
//!
//! 切分不变量: **跨单元数据边只能从供给节点发出**。
//! 计算节点之间的值/字符串边由并查集合并为同一单元, 因此任意计算节点的
//! 输入要么在本单元内 (单元内拓扑序保证先写后读), 要么来自供给节点
//! (prelude 单元先于全部计算单元执行 — 并发执行时每份槽位副本先跑 prelude)。
//! 供给节点 (ProtocolSource/Input/Custom/TextInput) 无图输入, 不参与合并,
//! 单个供给节点可喂多条独立路径 (跨单元扇出)。

use rustc_hash::FxHashMap;

use hir::TypedGraph;
use kind::NodeKind;
use plane::ValueMir;

/// 供给节点 — 无图输入的纯外部状态读 (ProtocolSource 读最新帧/文本,
/// Input/Custom 读前端回传, TextInput 读参数文本)
pub const fn is_provider(kind: &NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::ProtocolSource { .. }
            | NodeKind::Input
            | NodeKind::Custom { .. }
            | NodeKind::TextInput { .. }
    )
}

/// 并查集 (路径压缩)
struct Dsu {
    parent: Vec<u32>,
}

impl Dsu {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..u32::try_from(n).unwrap_or(u32::MAX)).collect(),
        }
    }

    fn find(&mut self, x: u32) -> u32 {
        let mut root = x;
        while self.parent[root as usize] != root {
            root = self.parent[root as usize];
        }
        let mut cur = x;
        while self.parent[cur as usize] != cur {
            let next = self.parent[cur as usize];
            self.parent[cur as usize] = root;
            cur = next;
        }
        root
    }

    fn union(&mut self, a: u32, b: u32) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            // 挂小下标到大下标, 保证 root 唯一性方向稳定
            let (lo, hi) = (ra.min(rb), ra.max(rb));
            self.parent[lo as usize] = hi;
        }
    }
}

/// 值平面连通分量切分
///
/// 返回 `unit_of`: 与 `mir.order` 等长 — 每个位置所属单元下标;
/// 供给节点恒为 0 (prelude), 计算节点按连通分量得 1..N
/// (分量编号按 mir.order 首次出现序分配, 保证编译结果确定性)。
/// 无计算节点时返回全 0 (整图 = prelude 单元)。
pub fn partition(g: &TypedGraph, mir: &ValueMir) -> Vec<u32> {
    let n = mir.order.len();
    let mut unit_of = vec![0u32; n];

    // 计算节点 id → mir.order 下标 (供给节点不入表)
    let mut pos_of_id: FxHashMap<&str, u32> = FxHashMap::default();
    for (i, &ix) in mir.order.iter().enumerate() {
        let Some(def) = g.graph[ix].value_def.as_ref() else {
            continue;
        };
        if !is_provider(&def.kind) {
            let pos = u32::try_from(i).unwrap_or(u32::MAX);
            pos_of_id.insert(g.id_of(ix), pos);
        }
    }

    // 计算节点间的边 (f32 + 字符串) → 合并分量
    let mut dsu = Dsu::new(n);
    let link = |tid: &str, sid: &str, dsu: &mut Dsu| {
        if let (Some(&t), Some(&s)) = (pos_of_id.get(tid), pos_of_id.get(sid)) {
            dsu.union(t, s);
        }
    };
    for (tid, ports) in &mir.input_index {
        for (sid, _) in ports.values() {
            link(tid, sid, &mut dsu);
        }
    }
    for (tid, ports) in &mir.string_input_index {
        for (sid, _) in ports.values() {
            link(tid, sid, &mut dsu);
        }
    }

    // 分量编号: 按 mir.order 首次出现序 (确定性)
    let mut comp_of_root: FxHashMap<u32, u32> = FxHashMap::default();
    let mut next_unit = 1u32;
    for (i, &ix) in mir.order.iter().enumerate() {
        if !pos_of_id.contains_key(g.id_of(ix)) {
            continue;
        }
        let root = dsu.find(u32::try_from(i).unwrap_or(u32::MAX));
        let unit = *comp_of_root.entry(root).or_insert_with(|| {
            let u = next_unit;
            next_unit += 1;
            u
        });
        unit_of[i] = unit;
    }
    unit_of
}

#[cfg(test)]
mod tests {
    use super::*;
    use kind::MathOp;
    use testkit::{edge, make_input, make_math, make_protocol_source};

    #[test]
    fn 独立链切分为不同单元_供给不参与合并() {
        // ps1 → m1 → m2; ps1 → m3 (ps1 供给喂两条独立链, 不合并 m1/m3)
        // input1 → m1 (m1/m3 仍独立 — input1 是供给)
        let g = TypedGraph::build(
            vec![
                make_protocol_source("ps1", "t1", "proto1", 1),
                make_input("input1", "t1"),
                make_math("m1", "t1", MathOp::Add, 1),
                make_math("m2", "t1", MathOp::Add, 1),
                make_math("m3", "t1", MathOp::Add, 1),
            ],
            vec![
                edge("e1", "ps1", "ch0", "m1", "in0"),
                edge("e2", "input1", "value", "m1", "in1"),
                edge("e3", "m1", "result", "m2", "in0"),
                edge("e4", "ps1", "ch0", "m3", "in0"),
            ],
        )
        .unwrap();
        let mir = plane::value_plane(&g).unwrap();
        let unit_of = partition(&g, &mir);
        // 供给 → prelude (0)
        for id in ["ps1", "input1"] {
            let pos = mir.order.iter().position(|&ix| g.id_of(ix) == id).unwrap();
            assert_eq!(unit_of[pos], 0, "{id} 应在 prelude");
        }
        // m1/m2 同链同单元; m3 独立单元
        let pos = |id: &str| mir.order.iter().position(|&ix| g.id_of(ix) == id).unwrap();
        assert_eq!(unit_of[pos("m1")], unit_of[pos("m2")], "链式节点应同单元");
        assert_ne!(unit_of[pos("m1")], unit_of[pos("m3")], "独立链应异单元");
        assert!(unit_of[pos("m1")] > 0 && unit_of[pos("m3")] > 0);
    }

    #[test]
    fn 无计算节点_整图为prelude() {
        let g = TypedGraph::build(vec![make_input("input1", "t1")], vec![]).unwrap();
        let mir = plane::value_plane(&g).unwrap();
        let unit_of = partition(&g, &mir);
        assert!(unit_of.iter().all(|&u| u == 0));
    }
}
