//! 节点图数据路由
//!
//! 管理节点之间的连接关系 (Edge), 根据连接将数据帧路由到目标显示控件。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 节点连接边
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: String,
    pub source: String,
    pub source_handle: String,
    pub target: String,
    pub target_handle: String,
}

/// 路由结果 — 某个目标节点应收到哪些通道的数据
#[derive(Debug, Clone)]
pub struct RoutedData {
    /// 目标节点 ID (显示控件 ID)
    pub target_node: String,
    /// 目标端口 ID (如 "CH0", "seg0")
    pub target_handle: String,
    /// 数据值
    pub value: f32,
}

/// 节点图 — 管理边集合, 提供数据路由功能
pub struct NodeGraph {
    edges: Vec<Edge>,
    /// 索引: source_node → [(source_handle, edge)]
    source_index: HashMap<String, Vec<(String, Edge)>>,
}

impl Default for NodeGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeGraph {
    pub fn new() -> Self {
        Self {
            edges: Vec::new(),
            source_index: HashMap::new(),
        }
    }

    /// 更新全部边 (替换)
    pub fn update_edges(&mut self, edges: Vec<Edge>) {
        self.edges = edges;
        self.rebuild_index();
    }

    /// 添加单条边
    pub fn add_edge(&mut self, edge: Edge) {
        let src = edge.source.clone();
        let handle = edge.source_handle.clone();
        self.edges.push(edge.clone());
        self.source_index
            .entry(src)
            .or_default()
            .push((handle, edge));
    }

    /// 移除边
    pub fn remove_edge(&mut self, edge_id: &str) {
        self.edges.retain(|e| e.id != edge_id);
        self.rebuild_index();
    }

    /// 获取所有边
    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// 获取连接到指定目标节点的所有边
    pub fn edges_to(&self, target: &str) -> Vec<&Edge> {
        self.edges.iter().filter(|e| e.target == target).collect()
    }

    /// 获取指定源节点的所有边
    pub fn edges_from(&self, source: &str) -> Vec<&Edge> {
        self.edges.iter().filter(|e| e.source == source).collect()
    }

    /// 路由数据帧 — 将帧中的每通道值分发到连接的目标节点
    ///
    /// 假设: source 节点为"通道源", source_handle 格式为 "ch{N}"
    ///       target_handle 格式取决于目标控件 (如 "CH0", "seg0")
    pub fn route_frame(&self, frame: &vofa_next_core::DataFrame) -> Vec<RoutedData> {
        let mut results = Vec::new();

        // 遍历每个通道, 查找是否有对应的源节点连接
        // source_handle 格式: "ch0", "ch1", ...
        for (ch_idx, &value) in frame.channels.iter().enumerate() {
            let source_handle = format!("ch{}", ch_idx);

            // 查找所有 source_handle == "chN" 的边
            // 遍历所有源节点 (因为通道源节点可能有多个实例)
            for edges in self.source_index.values() {
                for (handle, edge) in edges {
                    if handle == &source_handle {
                        results.push(RoutedData {
                            target_node: edge.target.clone(),
                            target_handle: edge.target_handle.clone(),
                            value,
                        });
                    }
                }
            }
        }

        results
    }

    /// 路由单个值 (用于输入控件值变化时推送)
    ///
    /// source = 控件节点 ID, source_handle = "value"
    pub fn route_value(&self, source: &str, value: f32) -> Vec<RoutedData> {
        let mut results = Vec::new();
        if let Some(edges) = self.source_index.get(source) {
            for (_handle, edge) in edges {
                results.push(RoutedData {
                    target_node: edge.target.clone(),
                    target_handle: edge.target_handle.clone(),
                    value,
                });
            }
        }
        results
    }

    /// 检测循环连接 (简单 DFS)
    pub fn has_cycle(&self) -> bool {
        let mut visited: HashMap<String, u8> = HashMap::new(); // 0=未访问, 1=访问中, 2=已完成

        fn dfs(node: &str, edges: &[Edge], visited: &mut HashMap<String, u8>) -> bool {
            match visited.get(node) {
                Some(&1) => return true,  // 发现环
                Some(&2) => return false, // 已完成
                _ => {}
            }
            visited.insert(node.to_string(), 1);
            for edge in edges {
                if edge.source == node
                    && dfs(&edge.target, edges, visited) {
                        return true;
                    }
            }
            visited.insert(node.to_string(), 2);
            false
        }

        for edge in &self.edges {
            if dfs(&edge.source, &self.edges, &mut visited) {
                return true;
            }
        }
        false
    }

    fn rebuild_index(&mut self) {
        self.source_index.clear();
        for edge in &self.edges {
            let src = edge.source.clone();
            let handle = edge.source_handle.clone();
            self.source_index
                .entry(src)
                .or_default()
                .push((handle, edge.clone()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vofa_next_core::DataFrame;

    #[test]
    fn test_empty_graph() {
        let graph = NodeGraph::new();
        let frame = DataFrame::new(vec![1.0, 2.0]);
        let routes = graph.route_frame(&frame);
        assert!(routes.is_empty());
    }

    #[test]
    fn test_single_connection() {
        let mut graph = NodeGraph::new();
        graph.add_edge(Edge {
            id: "e1".into(),
            source: "channel_source".into(),
            source_handle: "ch0".into(),
            target: "waveform1".into(),
            target_handle: "CH0".into(),
        });

        let frame = DataFrame::new(vec![42.0, 99.0]);
        let routes = graph.route_frame(&frame);
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].target_node, "waveform1");
        assert_eq!(routes[0].target_handle, "CH0");
        assert!((routes[0].value - 42.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_multi_connection_same_source() {
        let mut graph = NodeGraph::new();
        // 同一个通道源连接到多个显示控件
        graph.add_edge(Edge {
            id: "e1".into(),
            source: "ch_src".into(),
            source_handle: "ch0".into(),
            target: "waveform1".into(),
            target_handle: "CH0".into(),
        });
        graph.add_edge(Edge {
            id: "e2".into(),
            source: "ch_src".into(),
            source_handle: "ch0".into(),
            target: "pie1".into(),
            target_handle: "seg0".into(),
        });

        let frame = DataFrame::new(vec![55.0]);
        let routes = graph.route_frame(&frame);
        assert_eq!(routes.len(), 2);
        // 两个目标都收到值
        assert!(routes.iter().any(|r| r.target_node == "waveform1"));
        assert!(routes.iter().any(|r| r.target_node == "pie1"));
    }

    #[test]
    fn test_multi_channel_routing() {
        let mut graph = NodeGraph::new();
        graph.add_edge(Edge {
            id: "e1".into(),
            source: "ch_src".into(),
            source_handle: "ch0".into(),
            target: "waveform1".into(),
            target_handle: "CH0".into(),
        });
        graph.add_edge(Edge {
            id: "e2".into(),
            source: "ch_src".into(),
            source_handle: "ch1".into(),
            target: "waveform1".into(),
            target_handle: "CH1".into(),
        });

        let frame = DataFrame::new(vec![10.0, 20.0]);
        let routes = graph.route_frame(&frame);
        assert_eq!(routes.len(), 2);
        let ch0 = routes.iter().find(|r| r.target_handle == "CH0").unwrap();
        let ch1 = routes.iter().find(|r| r.target_handle == "CH1").unwrap();
        assert!((ch0.value - 10.0).abs() < f32::EPSILON);
        assert!((ch1.value - 20.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_route_value() {
        let mut graph = NodeGraph::new();
        graph.add_edge(Edge {
            id: "e1".into(),
            source: "knob1".into(),
            source_handle: "value".into(),
            target: "label1".into(),
            target_handle: "value".into(),
        });

        let routes = graph.route_value("knob1", 123.0);
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].target_node, "label1");
        assert!((routes[0].value - 123.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_no_route_for_unknown_source() {
        let graph = NodeGraph::new();
        let routes = graph.route_value("nonexistent", 1.0);
        assert!(routes.is_empty());
    }

    #[test]
    fn test_remove_edge() {
        let mut graph = NodeGraph::new();
        graph.add_edge(Edge {
            id: "e1".into(),
            source: "src".into(),
            source_handle: "ch0".into(),
            target: "tgt".into(),
            target_handle: "CH0".into(),
        });
        graph.add_edge(Edge {
            id: "e2".into(),
            source: "src".into(),
            source_handle: "ch1".into(),
            target: "tgt".into(),
            target_handle: "CH1".into(),
        });

        graph.remove_edge("e1");
        assert_eq!(graph.edges().len(), 1);
        assert_eq!(graph.edges()[0].id, "e2");
    }

    #[test]
    fn test_update_edges() {
        let mut graph = NodeGraph::new();
        graph.add_edge(Edge {
            id: "e1".into(),
            source: "src".into(),
            source_handle: "ch0".into(),
            target: "tgt".into(),
            target_handle: "CH0".into(),
        });

        graph.update_edges(vec![Edge {
            id: "e2".into(),
            source: "new_src".into(),
            source_handle: "ch0".into(),
            target: "new_tgt".into(),
            target_handle: "CH0".into(),
        }]);

        assert_eq!(graph.edges().len(), 1);
        assert_eq!(graph.edges()[0].id, "e2");
    }

    #[test]
    fn test_cycle_detection_no_cycle() {
        let mut graph = NodeGraph::new();
        graph.add_edge(Edge {
            id: "e1".into(),
            source: "a".into(),
            source_handle: "ch0".into(),
            target: "b".into(),
            target_handle: "CH0".into(),
        });
        graph.add_edge(Edge {
            id: "e2".into(),
            source: "b".into(),
            source_handle: "ch0".into(),
            target: "c".into(),
            target_handle: "CH0".into(),
        });
        assert!(!graph.has_cycle());
    }

    #[test]
    fn test_cycle_detection_with_cycle() {
        let mut graph = NodeGraph::new();
        graph.add_edge(Edge {
            id: "e1".into(),
            source: "a".into(),
            source_handle: "ch0".into(),
            target: "b".into(),
            target_handle: "CH0".into(),
        });
        graph.add_edge(Edge {
            id: "e2".into(),
            source: "b".into(),
            source_handle: "ch0".into(),
            target: "a".into(),
            target_handle: "CH0".into(),
        });
        assert!(graph.has_cycle());
    }

    #[test]
    fn test_edges_to() {
        let mut graph = NodeGraph::new();
        graph.add_edge(Edge {
            id: "e1".into(),
            source: "src1".into(),
            source_handle: "ch0".into(),
            target: "tgt1".into(),
            target_handle: "CH0".into(),
        });
        graph.add_edge(Edge {
            id: "e2".into(),
            source: "src2".into(),
            source_handle: "ch0".into(),
            target: "tgt1".into(),
            target_handle: "CH1".into(),
        });
        graph.add_edge(Edge {
            id: "e3".into(),
            source: "src3".into(),
            source_handle: "ch0".into(),
            target: "tgt2".into(),
            target_handle: "CH0".into(),
        });

        let edges = graph.edges_to("tgt1");
        assert_eq!(edges.len(), 2);
    }

    #[test]
    fn test_edges_from() {
        let mut graph = NodeGraph::new();
        graph.add_edge(Edge {
            id: "e1".into(),
            source: "src1".into(),
            source_handle: "ch0".into(),
            target: "tgt1".into(),
            target_handle: "CH0".into(),
        });
        graph.add_edge(Edge {
            id: "e2".into(),
            source: "src1".into(),
            source_handle: "ch1".into(),
            target: "tgt2".into(),
            target_handle: "CH0".into(),
        });
        graph.add_edge(Edge {
            id: "e3".into(),
            source: "src2".into(),
            source_handle: "ch0".into(),
            target: "tgt3".into(),
            target_handle: "CH0".into(),
        });

        let edges = graph.edges_from("src1");
        assert_eq!(edges.len(), 2);
    }
}
