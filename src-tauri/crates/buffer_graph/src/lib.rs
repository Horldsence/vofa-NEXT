//! # buffer_graph
//!
//! 节点图数据路由 — Edge / NodeGraph / RoutedData 三层结构。
//!
//! - [`Edge`][]: 节点连接边 (source/target + handle)
//! - [`NodeGraph`][]: 边集合管理 + 按 source/target 索引 + 数据帧路由
//! - [`RoutedData`][]: 路由结果 (目标节点 + 端口 + 值)

mod edge;
mod node_graph;
mod routed;

pub use edge::Edge;
pub use node_graph::NodeGraph;
pub use routed::RoutedData;
