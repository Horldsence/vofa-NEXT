//! MCP server 实现 — 工具箱抽象、工具 handler 与 HTTP 生命周期。
//!
//! 工具 handler 操作从 [`AppState`] 拆出的 [`Toolbox`] (各字段本就是
//! `Arc` 共享句柄,与 Tauri 管理的是同一份状态),避免 `app_state →
//! mcp_server` 循环依赖;图提交复用 [`cmd_graph::apply_tab_graph_parts`]。

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use app_state::AppState;
use buffer_raw::RawDataDirection;
use error::McpError;
use pipeline_data_plane::DataPlaneState;
use pipeline_data_plane::data_plane::{byte_router, frame_dispatch};
use pipeline_data_plane::decoder_feed::DecoderFeedCache;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerInfo};
use rmcp::{ServerHandler, schemars};
use serde::Deserialize;
use serde_json::{Value, json};
use tauri::AppHandle;
use vofa_core::Result as VofaResult;

/// MCP HTTP 端点路径 (外部客户端配置的 URL 需指向它)。
pub const MCP_ENDPOINT_PATH: &str = "/mcp";

/// MCP server 工具所需的应用状态切片 (全部为 `Arc` 共享句柄)。
#[derive(Clone)]
pub struct Toolbox {
    /// 传输注册表 (与 `AppState::transport` 同一实例)。
    pub transport: Arc<tokio::sync::Mutex<transport_core::TransportManager>>,
    /// 数据平面 (字节路由 / 缓冲 / 输出快照)。
    pub data_plane: DataPlaneState,
    /// 控件输入值表。
    pub input_values: Arc<parking_lot::Mutex<HashMap<String, f32>>>,
    /// tab 图表 (节点图提交)。
    pub graphs: Arc<parking_lot::Mutex<HashMap<String, node_engine::CompiledGraph>>>,
    /// 图版本号 (节点图提交)。
    pub graphs_version: Arc<AtomicU64>,
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

/// 应用错误 → MCP internal error 文本。
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
    #[rmcp::tool(description = "列出全部传输节点 (串口/TCP/UDP 等) 及其连接状态。返回 [{node_id, state}] 数组")]
    async fn list_transports(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let mgr = self.toolbox.transport.lock().await;
        let list: Vec<Value> = mgr
            .list_open()
            .into_iter()
            .map(|node_id| {
                json!({
                    "node_id": node_id,
                    "state": mgr.state(&node_id).map(|s| format!("{s:?}")).unwrap_or_default(),
                })
            })
            .collect();
        tool_result(json!({ "transports": list }))
    }

    /// 发送字节到指定传输节点。
    #[rmcp::tool(description = "向指定传输节点发送原始字节。data 为字节数组 (0-255)。返回发送字节数")]
    async fn send_bytes(
        &self,
        Parameters(params): Parameters<SendBytesParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let len = params.data.len();
        self.toolbox
            .transport
            .lock()
            .await
            .send(&params.node_id, &params.data)
            .map_err(internal)?;
        self.push_tx_raw(&params.node_id, &params.data);
        tool_result(json!({ "sent_bytes": len }))
    }

    /// 发送文本 (UTF-8 字符串)。
    #[rmcp::tool(description = "向指定传输节点发送 UTF-8 文本 (按字节原样发送, 不自动加换行)。返回发送字节数")]
    async fn send_string(
        &self,
        Parameters(params): Parameters<SendStringParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let bytes = params.text.as_bytes().to_vec();
        let len = bytes.len();
        self.toolbox
            .transport
            .lock()
            .await
            .send(&params.node_id, &bytes)
            .map_err(internal)?;
        self.push_tx_raw(&params.node_id, &bytes);
        tool_result(json!({ "sent_bytes": len }))
    }

    /// 字节注入 — 沿全局字节平面路由 (喂协议引擎 / FrameDecoder / Transport.tx)。
    #[rmcp::tool(description = "把字节从 source_node_id 注入全局字节平面, 路由到其下游 (协议解析/回环发送)。与设备无连接时也可用于协议调试。返回命中下游数量")]
    async fn inject_bytes(
        &self,
        Parameters(params): Parameters<InjectBytesParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let plane = self.toolbox.data_plane.clone();
        let hit = plane
            .byte_plan
            .lock()
            .routes_for(&params.source_node_id)
            .len();
        let mut cache = DecoderFeedCache::new();
        let summary = byte_router::route_bytes(
            &plane,
            Some(&self.app),
            &params.source_node_id,
            &params.data,
            0,
            &mut cache,
        )
        .await;
        if summary.decoders_fed {
            frame_dispatch::refresh_snapshot(&plane);
        }
        tool_result(json!({ "routed_targets": hit }))
    }

    /// 设置控件输入值 (Input/Slider/Knob 等 widget 的当前值)。
    #[rmcp::tool(description = "设置节点图输入控件的值 (widget_id 为控件节点 id)。立即生效并触发一次求值")]
    async fn set_input_value(
        &self,
        Parameters(params): Parameters<SetInputValueParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.toolbox
            .input_values
            .lock()
            .insert(params.widget_id.clone(), params.value);
        frame_dispatch::refresh_snapshot(&self.toolbox.data_plane);
        tool_result(json!({ "ok": true }))
    }

    /// 读取图输出快照 (全部节点输出端口的最新值)。
    #[rmcp::tool(description = "读取节点图输出快照: {widgetId: {portId: value}}。用于观察控件/波形/计算节点的实时输出")]
    async fn get_graph_outputs(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let snapshot = self.toolbox.data_plane.eval.output_snapshot.lock();
        let values = snapshot
            .values
            .iter()
            .map(|(widget, ports)| {
                (
                    widget.clone(),
                    ports.iter().map(|(k, v)| (k.clone(), *v)).collect::<Value>(),
                )
            })
            .collect::<Value>();
        tool_result(json!({ "tick": snapshot.tick, "outputs": values }))
    }

    /// 读取指定数据源 (协议节点 id) 的最近波形数据。
    #[rmcp::tool(description = "读取指定数据源 (协议/FrameDecoder 节点 id) 最近 count 个采样点的波形窗口, 含通道名与数值")]
    async fn get_recent_waveform(
        &self,
        Parameters(params): Parameters<WaveformParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let buf = self.toolbox.data_plane.buffer_for(&params.source);
        let window = buf.lock().get_recent(params.count.max(1) as usize);
        tool_result(&window)
    }

    /// 列出可读取的数据源 (全部缓冲区 key)。
    #[rmcp::tool(description = "列出存在波形缓冲的数据源 id (可配合 get_recent_waveform 使用)")]
    async fn list_data_sources(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let keys: Vec<String> = self
            .toolbox
            .data_plane
            .buffers
            .lock()
            .keys()
            .cloned()
            .collect();
        tool_result(json!({ "sources": keys }))
    }

    /// 列出已有节点图的 tab id。
    #[rmcp::tool(description = "列出已提交节点图的 tab id 列表 (配合 update_graph 使用)")]
    async fn list_tabs(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let tabs: Vec<String> = self.toolbox.graphs.lock().keys().cloned().collect();
        tool_result(json!({ "tabs": tabs }))
    }

    /// 提交 (替换) 指定 tab 的节点图 — 与前端提交同一路径, 界面实时同步。
    #[rmcp::tool(description = "替换指定 tab 的节点图。nodes/edges 与前端 NodeDef/Edge 格式一致;提交成功后前端界面实时刷新。返回派生端口表。编译失败 (环/端口域不匹配) 返回错误, 旧图保留")]
    async fn update_graph(
        &self,
        Parameters(params): Parameters<UpdateGraphParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let nodes: Vec<node_kind::NodeDef> = params
            .nodes
            .iter()
            .map(|n| serde_json::from_value(n.clone()))
            .collect::<std::result::Result<_, _>>()
            .map_err(internal)?;
        let edges: Vec<buffer_graph::Edge> = params
            .edges
            .iter()
            .map(|e| serde_json::from_value(e.clone()))
            .collect::<std::result::Result<_, _>>()
            .map_err(internal)?;

        let derived = cmd_graph::apply_tab_graph_parts(
            &self.toolbox.graphs,
            &self.toolbox.graphs_version,
            &self.toolbox.data_plane,
            Some(&self.app),
            params.tab_id.clone(),
            nodes,
            edges,
        )
        .await
        .map_err(internal)?;
        tool_result(&derived)
    }
}

impl VofaMcpServer {
    /// TX 字节进该源 raw 收集器 (与 `send_raw` 命令保持统计口径一致)。
    fn push_tx_raw(&self, node_id: &str, data: &[u8]) {
        self.toolbox
            .data_plane
            .raw_collector_for(node_id)
            .lock()
            .push_chunk(vofa_core::now_us(), RawDataDirection::Tx, data);
    }
}

#[rmcp::tool_handler]
impl ServerHandler for VofaMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("vofa-next", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "VOFA-NEXT 串口/波形调试上位机。可发送指令到设备 (send_string/send_bytes)、\
                 读取波形与图输出 (get_recent_waveform/get_graph_outputs)、\
                 修改节点图 (update_graph)。先用 list_transports/list_data_sources/list_tabs 了解可用资源。",
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
