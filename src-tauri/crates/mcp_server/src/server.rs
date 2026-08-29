//! MCP server 实现 — 工具箱抽象、工具 handler 与 HTTP 生命周期。
//!
//! 工具 handler 操作从 [`AppState`] 拆出的 [`Toolbox`] (各字段本就是
//! `Arc` 共享句柄,与 Tauri 管理的是同一份状态),避免 `app_state →
//! mcp_server` 循环依赖;图提交复用 [`cmd_graph::apply_tab_graph_parts`]。
//! 工具具体实现统一在 [`crate::tools`] (内置 AI 原生工具执行器共用),此处
//! 仅做参数包装与错误映射。

use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use app_state::AppState;
use can_types::CanFrame;
use error::McpError;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerInfo};
use rmcp::{schemars, ServerHandler};
use serde::Deserialize;
use serde_json::Value;
use tauri::AppHandle;
use vofa_core::Result as VofaResult;

use crate::tools;

/// MCP HTTP 端点路径 (外部客户端配置的 URL 需指向它)。
pub const MCP_ENDPOINT_PATH: &str = "/mcp";

/// MCP server 工具所需的应用状态切片 (全部为 `Arc` 共享句柄)。
#[derive(Clone)]
pub struct Toolbox {
    /// 传输注册表 (与 `AppState::transport` 同一实例)。
    pub transport: Arc<tokio::sync::Mutex<transport_core::TransportManager>>,
    /// 数据平面 (字节路由 / 缓冲 / 输出快照)。
    pub data_plane: pipeline_data_plane::DataPlaneState,
    /// 控件输入值表。
    pub input_values: Arc<parking_lot::Mutex<HashMap<String, f32>>>,
    /// tab 图表 (节点图提交)。
    pub graphs: Arc<parking_lot::Mutex<HashMap<String, node_engine::CompiledGraph>>>,
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

/// 发送字节的入参。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SendBytesParams {
    /// 目标传输节点 id。
    node_id: String,
    /// 字节数组 (0-255)。
    data: Vec<u8>,
}

/// 发送文本的入参。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SendStringParams {
    /// 目标传输节点 id。
    node_id: String,
    /// UTF-8 文本。
    text: String,
}

/// 字节注入入参。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct InjectBytesParams {
    /// 注入源节点 id (字节边起点)。
    source_node_id: String,
    /// 字节数组 (0-255)。
    data: Vec<u8>,
}

/// 输入控件赋值入参。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SetInputValueParams {
    /// 控件节点 id。
    widget_id: String,
    /// 目标值。
    value: f32,
}

/// 波形读取入参。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct WaveformParams {
    /// 数据源 (协议/FrameDecoder 节点 id)。
    source: String,
    /// 读取的最近采样点数。
    count: u32,
}

/// 时间窗波形读取入参。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct WaveformWindowParams {
    /// 数据源 (协议/FrameDecoder 节点 id)。
    source: String,
    /// 窗口起点 (相对最新时间戳的毫秒偏移, 负数=过去)。
    start_ms: i64,
    /// 窗口终点 (同上)。
    end_ms: i64,
}

/// 缓冲区信息入参。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct BufferInfoParams {
    /// 数据源节点 id。
    source: String,
}

/// 图更新参数 — nodes/edges 为前端同构的 JSON (`NodeDef` / `Edge`)。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct UpdateGraphParams {
    /// 目标 tab id (前端控件页 tab)。
    tab_id: String,
    /// 节点定义数组 (与前端 NodeDef 格式一致: id / tab_id / kind)。
    #[serde(default)]
    nodes: Vec<Value>,
    /// 边数组 (与前端 Edge 格式一致: from/to + 端口引用)。
    #[serde(default)]
    edges: Vec<Value>,
    /// widget 配置记录数组 ({id, kind, params}) — 提供时整体替换该 tab 的
    /// widget 配置 (画布可完整渲染), 缺省保留现状。
    #[serde(default)]
    widgets: Option<Vec<Value>>,
    /// 节点画布位置 ({node_id: {x, y}}) — 提供时合并进工作区位置表。
    #[serde(default)]
    positions: Option<std::collections::HashMap<String, Value>>,
}

/// 连线入参。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ConnectEdgeParams {
    /// 源节点 id。
    source: String,
    /// 目标节点 id。
    target: String,
    /// 归属 tab (缺省自动定位: 优先同时持有两端的 tab)。
    tab_id: Option<String>,
    /// 源端口 id (缺省按端口提示/节点类型补默认, 如 rx / out)。
    source_handle: Option<String>,
    /// 目标端口 id (缺省按端口提示/节点类型补默认, 如 in)。
    target_handle: Option<String>,
}

/// 删线入参 — edge_id 或 source/target 至少给一个。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DisconnectEdgeParams {
    /// 连线 id (优先精确匹配)。
    edge_id: Option<String>,
    /// 源节点 id (与 target 组合过滤, 可只给一端)。
    source: Option<String>,
    /// 目标节点 id。
    target: Option<String>,
}

/// CAN 帧读取入参。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CanFramesParams {
    /// 读取的最近帧条数 (上限 1000)。
    count: u32,
    /// 总线比特率 (用于负载百分比估算, 缺省 500k)。
    bitrate: Option<u32>,
}

/// CAN 帧发送入参。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SendCanFrameParams {
    /// 目标 Transport 节点 id。
    node_id: String,
    /// 编码用 Protocol 节点 id (缺省沿字节平面自动溯源第一个)。
    protocol_node: Option<String>,
    /// CAN 帧 (id 11/29 位;extended = 扩展帧;direction 通常 tx)。
    frame: CanFrameDto,
}

/// CAN 帧入参 DTO — 本地定义以派生 `JsonSchema` (`can_types::CanFrame` 无此派生)。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CanFrameDto {
    /// 帧 id (11 位标准帧或 29 位扩展帧)。
    id: u32,
    /// 是否扩展帧 (29 位 id)。
    #[serde(default)]
    extended: bool,
    /// 是否远程帧。
    #[serde(default)]
    rtr: bool,
    /// 数据字节 (最多 8 个)。
    #[serde(default)]
    data: Vec<u8>,
    /// 方向 ("tx"/"rx", 发送填 "tx")。
    #[serde(default)]
    direction: Option<String>,
}

impl From<CanFrameDto> for CanFrame {
    fn from(dto: CanFrameDto) -> Self {
        let direction = match dto.direction.as_deref() {
            Some("tx") | Some("Tx") | Some("TX") => can_types::CanDirection::Tx,
            _ => can_types::CanDirection::Rx,
        };
        Self {
            timestamp: vofa_core::now_us(),
            id: dto.id,
            extended: dto.extended,
            rtr: dto.rtr,
            dlc: dto.data.len().min(8) as u8,
            data: dto.data.into_iter().take(8).collect(),
            direction,
        }
    }
}

/// 逻辑分析数据读取入参。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct LogicParams {
    /// 读取的最近采样 / 事件条数 (上限 5000)。
    count: u32,
}

/// 原始字节读取入参。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct RawDataParams {
    /// 数据源节点 id (Transport 或 FrameDecoder)。
    source: String,
    /// 最大读取字节数 (上限 64KiB)。
    max_bytes: u32,
}

/// MCP server handler — 以宏生成工具路由与分发。
#[derive(Clone)]
pub struct VofaMcpServer {
    toolbox: Toolbox,
    app: AppHandle,
}

/// 序列化值的工具结果统一包装。
fn tool_result(value: impl serde::Serialize) -> Result<CallToolResult, rmcp::ErrorData> {
    let content = ContentBlock::json(value)?;
    Ok(CallToolResult::success(vec![content]))
}

/// 共享实现错误字符串 → MCP internal error。
fn internal(e: impl std::fmt::Display) -> rmcp::ErrorData {
    rmcp::ErrorData::internal_error(e.to_string(), None)
}

#[rmcp::tool_router]
impl VofaMcpServer {
    /// 构造 handler。
    pub const fn new(toolbox: Toolbox, app: AppHandle) -> Self {
        Self { toolbox, app }
    }

    /// 列出全部传输节点及其连接状态。
    #[rmcp::tool(
        description = "列出全部传输节点 (串口/TCP/UDP 等) 及其连接状态。返回 [{node_id, state}] 数组"
    )]
    async fn list_transports(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_result(tools::list_transports(&self.toolbox).await)
    }

    /// 列出可用串口。
    #[rmcp::tool(
        description = "列出系统可用串口 [{name, port_type, vid, pid, serial_number, manufacturer, product}]。连接串口前先用它确定端口名"
    )]
    async fn list_serial_ports(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::list_serial_ports()
            .map_err(internal)
            .and_then(tool_result)
    }

    /// 发送字节到指定传输节点。
    #[rmcp::tool(
        description = "向指定传输节点发送原始字节。data 为字节数组 (0-255)。返回发送字节数"
    )]
    async fn send_bytes(
        &self,
        Parameters(params): Parameters<SendBytesParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::send_bytes(&self.toolbox, &params.node_id, &params.data)
            .await
            .map_err(internal)
            .and_then(tool_result)
    }

    /// 发送文本 (UTF-8 字符串)。
    #[rmcp::tool(
        description = "向指定传输节点发送 UTF-8 文本 (按字节原样发送, 不自动加换行)。返回发送字节数"
    )]
    async fn send_string(
        &self,
        Parameters(params): Parameters<SendStringParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::send_string(&self.toolbox, &params.node_id, &params.text)
            .await
            .map_err(internal)
            .and_then(tool_result)
    }

    /// 字节注入 — 沿全局字节平面路由 (喂协议引擎 / FrameDecoder / Transport.tx)。
    #[rmcp::tool(
        description = "把字节从 source_node_id 注入全局字节平面, 路由到其下游 (协议解析/回环发送)。与设备无连接时也可用于协议调试。返回命中下游数量"
    )]
    async fn inject_bytes(
        &self,
        Parameters(params): Parameters<InjectBytesParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::inject_bytes(
            &self.toolbox,
            &self.app,
            &params.source_node_id,
            &params.data,
        )
        .await
        .map_err(internal)
        .and_then(tool_result)
    }

    /// 设置控件输入值 (Input/Slider/Knob 等 widget 的当前值)。
    #[rmcp::tool(
        description = "设置节点图输入控件的值 (widget_id 为控件节点 id)。立即生效并触发一次求值"
    )]
    async fn set_input_value(
        &self,
        Parameters(params): Parameters<SetInputValueParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_result(tools::set_input_value(
            &self.toolbox,
            &params.widget_id,
            params.value,
        ))
    }

    /// 读取图输出快照 (全部节点输出端口的最新值)。
    #[rmcp::tool(
        description = "读取节点图输出快照: {widgetId: {portId: value}}。用于观察控件/波形/计算节点的实时输出"
    )]
    async fn get_graph_outputs(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_result(tools::get_graph_outputs(&self.toolbox))
    }

    /// 读取指定数据源 (协议节点 id) 的最近波形数据。
    #[rmcp::tool(
        description = "读取指定数据源 (协议/FrameDecoder 节点 id) 最近 count 个采样点的波形窗口, 含通道名与数值"
    )]
    async fn get_recent_waveform(
        &self,
        Parameters(params): Parameters<WaveformParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::get_recent_waveform(&self.toolbox, &params.source, params.count)
            .map_err(internal)
            .and_then(tool_result)
    }

    /// 读取指定数据源时间窗内的波形。
    #[rmcp::tool(
        description = "读取指定数据源在时间窗口内的波形 (start_ms/end_ms 为相对最新时间戳的毫秒偏移, 负数=过去, 如 start=-1000/end=0 即最近 1 秒)"
    )]
    async fn get_waveform_window(
        &self,
        Parameters(params): Parameters<WaveformWindowParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::get_waveform_window(
            &self.toolbox,
            &params.source,
            params.start_ms,
            params.end_ms,
        )
        .map_err(internal)
        .and_then(tool_result)
    }

    /// 读取缓冲区信息。
    #[rmcp::tool(description = "读取指定数据源波形缓冲的通道数与点数 {channel_count, point_count}")]
    async fn get_buffer_info(
        &self,
        Parameters(params): Parameters<BufferInfoParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_result(tools::get_buffer_info(&self.toolbox, &params.source))
    }

    /// 列出可读取的数据源 (全部缓冲区 key)。
    #[rmcp::tool(description = "列出存在波形缓冲的数据源 id (可配合 get_recent_waveform 使用)")]
    async fn list_data_sources(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_result(tools::list_data_sources(&self.toolbox))
    }

    /// 列出已有节点图的 tab id。
    #[rmcp::tool(description = "列出已提交节点图的 tab id 列表 (配合 update_graph 使用)")]
    async fn list_tabs(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_result(tools::list_tabs(&self.toolbox))
    }

    /// 提交 (替换) 指定 tab 的节点图 — 与前端提交同一路径, 界面实时同步。
    #[rmcp::tool(
        description = "替换指定 tab 的节点图。nodes/edges 与前端 NodeDef/Edge 格式一致;widgets 可选, 为控件配置记录数组 [{id, kind, params}] (提供时画布可完整渲染控件), positions 可选为节点画布位置 {node_id: {x, y}}。提交成功后前端界面实时刷新。返回派生端口表。编译失败 (环/端口域不匹配) 返回错误, 旧图保留"
    )]
    async fn update_graph(
        &self,
        Parameters(params): Parameters<UpdateGraphParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::update_graph(
            &self.toolbox,
            &self.app,
            &params.tab_id,
            params.nodes,
            params.edges,
            params.widgets,
            params.positions,
        )
        .await
        .map_err(internal)
        .and_then(tool_result)
    }

    /// 读取最近 CAN 帧与负载统计。
    #[rmcp::tool(
        description = "读取最近 CAN 帧 [{timestamp, id, extended, dlc, data, direction}] 与总线负载统计 {fps, load_ratio}"
    )]
    async fn get_can_frames(
        &self,
        Parameters(params): Parameters<CanFramesParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_result(tools::get_can_frames(
            &self.toolbox,
            params.count,
            params.bitrate,
        ))
    }

    /// 连线 (后端编译校验 — 域不匹配/成环直接报错, 不建边)。
    #[rmcp::tool(
        description = "在两个节点端口间建立连线。handle 缺省时自动补默认端口;RawData 控件目标自动改写 src: 端口。编译失败 (端口域不匹配/成环) 返回真实原因且不建边。成功返回 {edge_id} 并实时同步到界面"
    )]
    async fn connect_edge(
        &self,
        Parameters(params): Parameters<ConnectEdgeParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::connect_edge(
            &self.toolbox,
            &self.app,
            params.tab_id,
            &params.source,
            &params.target,
            params.source_handle,
            params.target_handle,
        )
        .await
        .map_err(internal)
        .and_then(tool_result)
    }

    /// 删线。
    #[rmcp::tool(
        description = "删除连线: 给 edge_id 精确删除, 或给 source/target (可只给一端) 删除第一条匹配。成功返回被删边信息并实时同步到界面"
    )]
    async fn disconnect_edge(
        &self,
        Parameters(params): Parameters<DisconnectEdgeParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::disconnect_edge(
            &self.toolbox,
            &self.app,
            params.edge_id,
            params.source,
            params.target,
        )
        .await
        .map_err(internal)
        .and_then(tool_result)
    }

    /// 发送 CAN 帧。
    #[rmcp::tool(
        description = "发送 CAN 帧 (经 CAN 协议节点 encode_can 编码)。protocol_node 缺省时沿字节平面自动溯源该传输下游的第一个 Protocol 节点"
    )]
    async fn send_can_frame(
        &self,
        Parameters(params): Parameters<SendCanFrameParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tools::send_can_frame(
            &self.toolbox,
            &params.node_id,
            params.protocol_node,
            params.frame.into(),
        )
        .await
        .map_err(internal)
        .and_then(tool_result)
    }

    /// 读取逻辑分析数据。
    #[rmcp::tool(
        description = "读取逻辑分析仪最近采样与解码事件 (UART/I2C/SPI 等) {samples, decoded_events}"
    )]
    async fn get_logic_data(
        &self,
        Parameters(params): Parameters<LogicParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_result(tools::get_logic_data(&self.toolbox, params.count))
    }

    /// 读取最近原始字节。
    #[rmcp::tool(
        description = "读取指定源 (Transport/FrameDecoder 节点 id) 最近收发的原始字节 (hex 编码, 含方向与时间戳)"
    )]
    async fn get_raw_data(
        &self,
        Parameters(params): Parameters<RawDataParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_result(tools::get_raw_data(
            &self.toolbox,
            &params.source,
            params.max_bytes,
        ))
    }
}

#[rmcp::tool_handler]
impl ServerHandler for VofaMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("vofa-next", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "VOFA-NEXT 串口/波形调试上位机。可发送指令到设备 (send_string/send_bytes/send_can_frame)、\
                 读取波形与图输出 (get_recent_waveform/get_graph_outputs)、\
                 修改节点图 (update_graph 整图替换 / connect_edge+disconnect_edge 增量连线, \
                 编译校验失败会返回真实原因)。先用 list_transports/list_data_sources/list_tabs 了解可用资源。",
            )
    }
}

/// 正在运行的 MCP server 句柄 — 显式 [`McpServerHandle::stop`] 触发优雅关闭。
pub struct McpServerHandle {
    /// 实际监听端口。
    pub port: u16,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    done_rx: tokio::sync::oneshot::Receiver<std::io::Result<()>>,
}

impl McpServerHandle {
    /// 优雅停止 (幂等)。
    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }

    /// 非阻塞检查 server 是否在运行 (内部任务出错则返回错误)。
    ///
    /// # Errors
    /// axum serve 任务以错误退出时返回 [`McpError::ServerStart`]。
    pub fn check_running(&mut self) -> VofaResult<bool> {
        match self.done_rx.try_recv() {
            Ok(Ok(())) => Ok(false),
            Ok(Err(source)) => Err(McpError::ServerStart {
                port: self.port,
                source: Box::new(source),
            }
            .into()),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => Ok(true),
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => Ok(false),
        }
    }
}

/// 在 `127.0.0.1:{port}` 启动 MCP streamable-http server。
///
/// # Errors
/// 端口占用等 bind 失败返回 [`McpError::ServerStart`]。
pub async fn start(toolbox: Toolbox, app: AppHandle, port: u16) -> VofaResult<McpServerHandle> {
    let service_factory = move || Ok(VofaMcpServer::new(toolbox.clone(), app.clone()));
    let session_manager = Arc::new(
        rmcp::transport::streamable_http_server::session::local::LocalSessionManager::default(),
    );
    let service = rmcp::transport::StreamableHttpService::new(
        service_factory,
        session_manager,
        rmcp::transport::StreamableHttpServerConfig::default(),
    );

    let router = axum::Router::new().route_service(MCP_ENDPOINT_PATH, service);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|source| McpError::ServerStart {
            port,
            source: Box::new(source),
        })?;
    let actual_port = listener.local_addr().ok().map_or(port, |a| a.port());

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let (done_tx, done_rx) = tokio::sync::oneshot::channel::<std::io::Result<()>>();
    tokio::spawn(async move {
        let server = axum::serve(listener, router).with_graceful_shutdown(async {
            let _ = shutdown_rx.await;
        });
        let _ = done_tx.send(server.await);
    });

    log::info!("MCP server 已启动: http://127.0.0.1:{actual_port}{MCP_ENDPOINT_PATH}");
    Ok(McpServerHandle {
        port: actual_port,
        shutdown_tx: Some(shutdown_tx),
        done_rx,
    })
}
