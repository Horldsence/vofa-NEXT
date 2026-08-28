use std::path::PathBuf;
use std::sync::Arc;

use ai_chat::{
    AiChatEvent, ChatPayload, ChatTaskRegistry, EventSink, GenaiTurnProvider, ToolExecutor,
    TurnRecorder, run_chat,
};
use ai_provider::{AdapterInfo, AiProviderConfig, ToolSpecDto, validate_config};
use ai_session::{ChatSession, SessionMeta, SessionStore, ViewItemDto, ViewRoleDto, derive_history};
use app_state::AppState;
use error::{McpError, Result};
use mcp_client::{McpManager, McpServerConfig, McpToolInfo};
use mcp_server::{McpServerHandle, Toolbox};
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager, State, ipc::Channel};

/// AI 功能全局状态 (Tauri managed)。
pub struct AiState {
    /// 对话任务取消注册表。
    registry: Arc<ChatTaskRegistry>,
    /// 对话会话存储 (多会话 + 历史持久化, 所有权在后端)。
    sessions: Arc<SessionStore>,
    /// 外部 MCP server 连接管理器。
    mcp: Arc<McpManager>,
    /// 聚合工具缓存 — 由 `mcp_list_tools` 刷新, 对话发送时取快照。
    tool_cache: Mutex<Vec<McpToolInfo>>,
    /// 本地 MCP server 句柄。
    server: Mutex<Option<McpServerHandle>>,
}

impl AiState {
    /// 从 app config dir 构造 (加载已配置的外部 MCP server 列表与会话历史;
    /// 配置文件损坏时按空配置启动, 不阻塞应用)。
    pub fn new(config_dir: PathBuf) -> Self {
        let mcp = McpManager::load(&config_dir).unwrap_or_else(|e| {
            log::warn!("MCP 配置加载失败, 按空配置启动: {e}");
            McpManager::empty(&config_dir)
        });
        let sessions = SessionStore::load(&config_dir).unwrap_or_else(|e| {
            log::warn!("AI 会话加载失败, 按空会话启动: {e}");
            SessionStore::empty(&config_dir)
        });
        Self {
            registry: Arc::new(ChatTaskRegistry::default()),
            sessions: Arc::new(sessions),
            mcp: Arc::new(mcp),
            tool_cache: Mutex::new(Vec::new()),
            server: Mutex::new(None),
        }
    }
}

/// MCP 聚合工具执行器 — 对话循环调用外部 MCP 工具的桥梁。
struct McpToolExecutor {
    tools: Vec<McpToolInfo>,
    mcp: Arc<McpManager>,
}

#[async_trait::async_trait]
impl ToolExecutor for McpToolExecutor {
    fn tools(&self) -> Vec<ToolSpecDto> {
        self.tools
            .iter()
            .map(|t| ToolSpecDto {
                name: t.prefixed_name.clone(),
                description: if t.description.is_empty() {
                    format!("{} 的工具 (server: {})", t.name, t.server_name)
                } else {
                    format!("{} (server: {})", t.description, t.server_name)
                },
                input_schema: t.input_schema.clone(),
            })
            .collect()
    }

    async fn call(&self, name: &str, arguments: Value) -> Result<String> {
        self.mcp.call_by_prefixed(name, arguments).await
    }
}

/// 无工具执行器 — 纯对话。
struct NoopExecutor;

#[async_trait::async_trait]
impl ToolExecutor for NoopExecutor {
    fn tools(&self) -> Vec<ToolSpecDto> {
        Vec::new()
    }

    async fn call(&self, name: &str, _arguments: Value) -> Result<String> {
        Err(McpError::ToolFailed {
            tool: name.to_string(),
            details: "本次对话未启用 MCP 工具".to_string(),
        }
        .into())
    }
}

// ============ AI 对话 ============

/// 支持的 LLM provider 适配器清单 (设置 UI 下拉)。
#[tauri::command]
pub fn ai_list_providers() -> Vec<AdapterInfo> {
    ai_provider::list_adapters().to_vec()
}

/// 发起一次对话 (可含多轮工具调用), 增量事件经 Channel 推送。
///
/// 会话所有权在后端:`text` 非空时先追加用户条目, `regenerate` 时截掉
/// 最后一条用户条目之后的待重试回合;LLM 历史由会话条目派生, 回合终态后
/// 产物落盘。返回 task_id 供 `ai_chat_cancel` 取消。错误事件同样走 Channel,
/// 命令本身只在配置/参数非法时失败。
///
/// # Errors
/// 配置校验失败 (缺 key / 未知适配器 / 缺模型名) 或会话落盘失败。
#[tauri::command]
pub async fn ai_chat_send(
    state: State<'_, AiState>,
    session_id: String,
    text: Option<String>,
    regenerate: bool,
    config: AiProviderConfig,
    system: Option<String>,
    max_tool_rounds: u32,
    use_mcp_tools: bool,
    on_event: Channel<AiChatEvent>,
) -> Result<String> {
    validate_config(&config)?;

    // 回合前置: 先落盘用户输入 / 截断待重试回合, 派生的历史才是权威上下文
    if regenerate {
        state.sessions.truncate_after_last_user(&session_id)?;
    } else if let Some(body) = text.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        state.sessions.append_items(
            &session_id,
            vec![ViewItemDto {
                role: ViewRoleDto::User,
                text: body.to_string(),
                tools: None,
                error: None,
            }],
        )?;
    }
    let history = derive_history(
        &state
            .sessions
            .get(&session_id)
            .map(|session| session.items)
            .unwrap_or_default(),
    );

    let (task_id, cancel_rx) = state.registry.register();
    let registry = Arc::clone(&state.registry);
    let sessions = Arc::clone(&state.sessions);
    let mcp = Arc::clone(&state.mcp);
    let tools = if use_mcp_tools {
        state.tool_cache.lock().clone()
    } else {
        Vec::new()
    };

    let spawned_task_id = task_id.clone();
    tauri::async_runtime::spawn(async move {
        // 事件双路: 转发前端流式渲染 + 记录器聚合 (终态后落盘)
        let recorder = Arc::new(Mutex::new(TurnRecorder::new()));
        let recorder_for_sink = Arc::clone(&recorder);
        let sink: EventSink = Arc::new(move |event| {
            recorder_for_sink.lock().record(&event);
            let _ = on_event.send(event);
        });
        let provider = GenaiTurnProvider;
        let use_tools = !tools.is_empty();
        let mcp_executor = McpToolExecutor { tools, mcp };
        let noop_executor = NoopExecutor;
        let executor: &dyn ToolExecutor = if use_tools {
            &mcp_executor
        } else {
            &noop_executor
        };
        let payload = ChatPayload {
            config,
            system,
            messages: history,
            max_tool_rounds,
        };
        if let Err(e) = run_chat(payload, &provider, executor, cancel_rx, Arc::clone(&sink)).await {
            // 取消/超轮次的专用事件已在循环内发出, 这里只记日志
            log::warn!("AI 对话任务结束 (含错误): {e}");
        }
        // 回合收束: 取消 / 错误路径同样沉淀部分结果
        let items = recorder.lock().finish();
        if let Err(e) = sessions.append_items(&session_id, items) {
            log::warn!("会话条目落盘失败: {e}");
        }
        registry.remove(&spawned_task_id);
    });
    Ok(task_id)
}

/// 取消进行中的对话任务;返回是否存在该任务。
#[tauri::command]
pub fn ai_chat_cancel(state: State<'_, AiState>, task_id: String) -> bool {
    state.registry.cancel(&task_id)
}

// ============ 对话会话 (后端持有, 前端薄视图) ============

/// 全部会话摘要。
#[tauri::command]
pub fn chat_list_sessions(state: State<'_, AiState>) -> Vec<SessionMeta> {
    state.sessions.list_metas()
}

/// 新建会话。
///
/// # Errors
/// 落盘失败。
#[tauri::command]
pub async fn chat_create_session(state: State<'_, AiState>, title: String) -> Result<ChatSession> {
    state.sessions.create(&title)
}

/// 读取单个会话 (含全部条目);不存在返回 None。
#[tauri::command]
pub fn chat_get_session(state: State<'_, AiState>, session_id: String) -> Option<ChatSession> {
    state.sessions.get(&session_id)
}

/// 重命名会话。
///
/// # Errors
/// 会话不存在或落盘失败。
#[tauri::command]
pub async fn chat_rename_session(
    state: State<'_, AiState>,
    session_id: String,
    title: String,
) -> Result<()> {
    state.sessions.rename(&session_id, &title)
}

/// 删除会话 (不存在时静默)。
///
/// # Errors
/// 落盘失败。
#[tauri::command]
pub async fn chat_delete_session(state: State<'_, AiState>, session_id: String) -> Result<()> {
    state.sessions.remove(&session_id)
}

/// 清空会话条目 (保留会话本身)。
///
/// # Errors
/// 落盘失败。
#[tauri::command]
pub async fn chat_clear_session(state: State<'_, AiState>, session_id: String) -> Result<()> {
    state.sessions.clear_items(&session_id)
}

// ============ MCP client (外部 server) ============

/// 全部外部 MCP server 配置。
#[tauri::command]
pub fn mcp_list_servers(state: State<'_, AiState>) -> Vec<McpServerConfig> {
    state.mcp.list_servers()
}

/// 新增外部 MCP server 配置。
///
/// # Errors
/// id 重复 / 配置非法 / 写盘失败。
#[tauri::command]
pub async fn mcp_add_server(state: State<'_, AiState>, config: McpServerConfig) -> Result<()> {
    state.mcp.add_server(config)
}

/// 删除外部 MCP server 配置 (同时断连)。
#[tauri::command]
pub fn mcp_remove_server(state: State<'_, AiState>, id: String) {
    state.mcp.remove_server(&id);
}

/// 启用 / 禁用外部 MCP server。
///
/// # Errors
/// id 不存在或写盘失败。
#[tauri::command]
pub async fn mcp_set_server_enabled(
    state: State<'_, AiState>,
    id: String,
    enabled: bool,
) -> Result<()> {
    state.mcp.set_enabled(&id, enabled)
}

/// 刷新聚合工具列表 (自动连接已启用但未连接的 server) 并更新缓存。
#[tauri::command]
pub async fn mcp_list_tools(state: State<'_, AiState>) -> Result<Vec<McpToolInfo>> {
    let tools = state.mcp.list_tools().await;
    state.tool_cache.lock().clone_from(&tools);
    Ok(tools)
}

/// 当前各 server 的连接状态 [(server_id, connected)]。
#[tauri::command]
pub fn mcp_connection_states(state: State<'_, AiState>) -> Vec<(String, bool)> {
    state.mcp.connection_states()
}

/// 手动调用一个聚合工具 (前缀名)。
///
/// # Errors
/// 工具未注册或远端调用失败。
#[tauri::command]
pub async fn mcp_call_tool(
    state: State<'_, AiState>,
    name: String,
    arguments: Value,
) -> Result<String> {
    state.mcp.call_by_prefixed(&name, arguments).await
}

// ============ MCP server (本应用能力暴露) ============

/// 本地 MCP server 状态。
#[derive(Debug, Serialize)]
pub struct McpServerStatus {
    /// 是否在运行。
    pub running: bool,
    /// 运行端口 (未运行为 null)。
    pub port: Option<u16>,
}

/// 查询本地 MCP server 状态。
#[tauri::command]
pub fn mcp_server_status(state: State<'_, AiState>) -> McpServerStatus {
    let mut guard = state.server.lock();
    if let Some(handle) = guard.as_mut() {
        if matches!(handle.check_running(), Ok(true)) {
            return McpServerStatus {
                running: true,
                port: Some(handle.port),
            };
        }
    }
    McpServerStatus {
        running: false,
        port: None,
    }
}

/// 启动本地 MCP server (已运行则直接返回当前端口)。
///
/// # Errors
/// 端口被占用 ([`McpError::ServerStart`])。
#[tauri::command]
pub async fn mcp_server_start(
    state: State<'_, AiState>,
    app: AppHandle,
    port: u16,
) -> Result<u16> {
    {
        let mut guard = state.server.lock();
        if let Some(handle) = guard.as_mut() {
            if matches!(handle.check_running(), Ok(true)) {
                return Ok(handle.port);
            }
        }
        *guard = None;
    }

    let app_state: State<AppState> = app.state();
    let toolbox = Toolbox::from_state(&app_state);
    let handle = mcp_server::start(toolbox, app, port).await?;
    let bound = handle.port;
    *state.server.lock() = Some(handle);
    Ok(bound)
}

/// 停止本地 MCP server (未运行时静默)。
#[tauri::command]
pub fn mcp_server_stop(state: State<'_, AiState>) {
    let taken = state.server.lock().take();
    if let Some(mut handle) = taken {
        handle.stop();
    }
}
