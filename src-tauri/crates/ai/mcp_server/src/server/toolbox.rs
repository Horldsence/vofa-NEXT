//! MCP server 工具所需的应用状态切片 (全部为 `Arc` 共享句柄)

use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use app_state::AppState;

/// MCP HTTP 端点路径 (外部客户端配置的 URL 需指向它)。
pub const MCP_ENDPOINT_PATH: &str = "/mcp";

/// MCP server 工具所需的应用状态切片 (全部为 `Arc` 共享句柄)。
#[derive(Clone)]
pub struct Toolbox {
    /// 传输注册表 (与 `AppState::transport` 同一实例)。
    pub transport: Arc<tokio::sync::Mutex<transport_core::TransportManager>>,
    /// 数据平面 (字节路由 / 缓冲 / 输出快照)。
    pub data_plane: data_plane::DataPlaneState,
    /// 控件输入值表。
    pub input_values: Arc<parking_lot::RwLock<HashMap<String, f32>>>,
    /// tab 图表 (节点图提交)。
    pub graphs: Arc<parking_lot::Mutex<HashMap<String, engine::CompiledGraph>>>,
    /// 图版本号 (节点图提交)。
    pub graphs_version: Arc<AtomicU64>,
    /// 源图存储 (连线拓扑权威 — connect_edge/disconnect_edge op 与 graph:source 事件)。
    pub source_graphs: app_state::SourceGraphs,
    /// 工作区存储 (widget 配置记录 / 画布位置 / tab 元数据 — 随图提交原子更新)。
    pub workspace: app_state::WorkspaceState,
    /// CAN 帧缓冲区。
    pub can_buffer: Arc<parking_lot::Mutex<can_types::CanBuffer>>,
    /// CAN 负载统计器 (滑动窗口)。
    pub can_load_stats: Arc<parking_lot::Mutex<can_types::CanLoadStats>>,
    /// 逻辑采样缓冲区。
    pub logic_buffer: Arc<parking_lot::Mutex<logic_types::LogicBuffer>>,
    /// 解码事件缓冲区。
    pub decoded_buffer: Arc<parking_lot::Mutex<logic_types::DecodedBuffer>>,
}

impl Toolbox {
    /// 从 Tauri 管理的 [`AppState`] 提取共享句柄。
    pub fn from_state(state: &AppState) -> Self {
        Self {
            transport: Arc::clone(&state.transport),
            data_plane: state.data_plane.clone(),
            input_values: Arc::clone(&state.input_values),
            graphs: Arc::clone(&state.graphs),
            graphs_version: Arc::clone(&state.graphs_version),
            source_graphs: Arc::clone(&state.source_graphs),
            workspace: Arc::clone(&state.workspace),
            can_buffer: Arc::clone(&state.can_buffer),
            can_load_stats: Arc::clone(&state.can_load_stats),
            logic_buffer: Arc::clone(&state.logic_buffer),
            decoded_buffer: Arc::clone(&state.decoded_buffer),
        }
    }
}
