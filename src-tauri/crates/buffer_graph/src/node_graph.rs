//! `NodeGraph` 节点图 — 边集合管理 + 索引 + 数据帧路由 + 循环检测

use std::collections::HashMap;

use vofa_core::DataFrame;

use crate::{Edge, RoutedData};

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
    pub fn route_frame(&self, frame: &DataFrame) -> Vec<RoutedData> {
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
                Some(&1) => return true, // 发现环
                Some(&2) => return false, // 已完成
                _ => {}
            }
            visited.insert(node.to_string(), 1);
            for edge in edges {
                if edge.source == node && dfs(&edge.target, edges, visited) {
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