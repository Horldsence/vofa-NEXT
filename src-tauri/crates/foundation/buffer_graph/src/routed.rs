//! `RoutedData` 路由结果

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

impl RoutedData {
    /// 构造一个新的路由结果
    pub fn new(
        target_node: impl Into<String>,
        target_handle: impl Into<String>,
        value: f32,
    ) -> Self {
        Self {
            target_node: target_node.into(),
            target_handle: target_handle.into(),
            value,
        }
    }
}
