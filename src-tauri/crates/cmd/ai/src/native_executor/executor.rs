//! 原生工具执行器本体 — 后端直连优先, 前端托管兜底

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chat::ToolExecutor;
use error::Result;
use mcp_server::tools;
use mcp_server::Toolbox;
use parking_lot::Mutex;
use provider::ToolSpecDto;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

use super::args::{
    arg_f64, arg_i64, arg_opt_str, arg_opt_u32, arg_str, arg_vec_u8, parse_can_frame, shared,
    tool_failed, value_to_content,
};
use super::specs::{native_tool_specs, BACKEND_TOOLS, FRONTEND_TOOLS};
use crate::skills::{self, Lang};

/// 前端托管工具调用事件名 (前端 `toolHost.ts` 监听)。
pub const AI_TOOL_INVOKE_EVENT: &str = "ai_tool_invoke";

/// 前端托管工具执行超时。
const FRONTEND_TOOL_TIMEOUT: Duration = Duration::from_secs(15);

/// 前端托管工具调用回执。
pub enum ToolOutcome {
    /// 成功 (工具结果字符串)。
    Ok(String),
    /// 失败 (错误描述)。
    Err(String),
}

/// pending 前端调用注册表 — call_id → 回执发送端 (`ai_tool_resolve` 消费)。
pub type PendingCalls = Arc<Mutex<HashMap<String, oneshot::Sender<ToolOutcome>>>>;

/// 原生工具执行器 — 内置 AI 调用软件自有能力的桥梁。
pub struct NativeToolExecutor {
    toolbox: Toolbox,
    app: AppHandle,
    pending: PendingCalls,
    lang: Lang,
}

impl NativeToolExecutor {
    /// 构造 (toolbox 从 `AppState` 提取, pending 注册表由 `AiState` 持有)。
    pub const fn new(toolbox: Toolbox, app: AppHandle, pending: PendingCalls, lang: Lang) -> Self {
        Self {
            toolbox,
            app,
            pending,
            lang,
        }
    }

    /// 是否处理该工具 (内置优先于外部 MCP)。
    pub fn handles(name: &str) -> bool {
        BACKEND_TOOLS.contains(&name) || FRONTEND_TOOLS.contains(&name)
    }

    /// 前端托管调用: 发事件 + 等回执 (超时兜底)。
    async fn call_frontend(&self, name: &str, arguments: Value) -> Result<String> {
        let call_id = tools::next_call_id();
        let (tx, rx) = oneshot::channel();
        self.pending.lock().insert(call_id.clone(), tx);

        let payload = json!({ "call_id": call_id, "name": name, "arguments": arguments });
        if let Err(e) = self.app.emit(AI_TOOL_INVOKE_EVENT, payload) {
            self.pending.lock().remove(&call_id);
            return Err(tool_failed(name, format!("事件派发失败: {e}")));
        }

        match tokio::time::timeout(FRONTEND_TOOL_TIMEOUT, rx).await {
            Ok(Ok(ToolOutcome::Ok(content))) => Ok(content),
            Ok(Ok(ToolOutcome::Err(details))) => Err(tool_failed(name, details)),
            Ok(Err(_dropped)) => {
                self.pending.lock().remove(&call_id);
                Err(tool_failed(name, "前端未回执 (界面不可用)"))
            }
            Err(_timeout) => {
                self.pending.lock().remove(&call_id);
                Err(tool_failed(name, "前端执行超时 (15s)"))
            }
        }
    }

    /// 后端直连分发 — 返回 None 表示非后端工具 (交由前端托管路径)。
    async fn call_backend(&self, name: &str, args: &Value) -> Result<Option<String>> {
        let tb = &self.toolbox;
        let out = match name {
            "list_transports" => tools::list_transports(tb).await,
            "list_serial_ports" => shared(name, tools::list_serial_ports())?,
            "send_bytes" => {
                let node_id = arg_str(name, args, "node_id")?;
                let data = arg_vec_u8(name, args, "data")?;
                shared(name, tools::send_bytes(tb, node_id, &data).await)?
            }
            "send_string" => {
                let node_id = arg_str(name, args, "node_id")?;
                let text = arg_str(name, args, "text")?;
                shared(name, tools::send_string(tb, node_id, text).await)?
            }
            "send_can_frame" => {
                let node_id = arg_str(name, args, "node_id")?;
                let protocol_node = arg_opt_str(args, "protocol_node").map(str::to_string);
                let frame = parse_can_frame(name, args)?;
                shared(
                    name,
                    tools::send_can_frame(tb, node_id, protocol_node, frame).await,
                )?
            }
            "set_input_value" => {
                let widget_id = arg_str(name, args, "widget_id")?;
                #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                // 图求值内部是 f32, f64→f32 精度损失可接受
                let value = arg_f64(name, args, "value")? as f32;
                tools::set_input_value(tb, widget_id, value)
            }
            "inject_bytes" => {
                let source = arg_str(name, args, "source_node_id")?;
                let data = arg_vec_u8(name, args, "data")?;
                shared(
                    name,
                    tools::inject_bytes(tb, &self.app, source, &data).await,
                )?
            }
            "get_graph_outputs" => tools::get_graph_outputs(tb),
            "get_recent_waveform" => {
                let source = arg_str(name, args, "source")?;
                let count = arg_opt_u32(args, "count").unwrap_or(100);
                shared(name, tools::get_recent_waveform(tb, source, count))?
            }
            "get_waveform_window" => {
                let source = arg_str(name, args, "source")?;
                let start = arg_i64(name, args, "start_ms")?;
                let end = arg_i64(name, args, "end_ms")?;
                shared(name, tools::get_waveform_window(tb, source, start, end))?
            }
            "get_buffer_info" => {
                let source = arg_str(name, args, "source")?;
                tools::get_buffer_info(tb, source)
            }
            "list_data_sources" => tools::list_data_sources(tb),
            "get_can_frames" => {
                let count = arg_opt_u32(args, "count").unwrap_or(100);
                let bitrate = arg_opt_u32(args, "bitrate");
                tools::get_can_frames(tb, count, bitrate)
            }
            "get_logic_data" => {
                let count = arg_opt_u32(args, "count").unwrap_or(200);
                tools::get_logic_data(tb, count)
            }
            "get_raw_data" => {
                let source = arg_str(name, args, "source")?;
                let max_bytes = arg_opt_u32(args, "max_bytes").unwrap_or(4096);
                tools::get_raw_data(tb, source, max_bytes)
            }
            "read_skill" => {
                let skill_id = arg_str(name, args, "skill_id")?;
                let lang = arg_opt_str(args, "lang").map_or(self.lang, Lang::parse);
                return skills::read_skill(skill_id, lang).map(Some);
            }
            // 连线拓扑 — 后端权威实现 (编译失败返回真实原因, 画布经 graph:source 收敛)
            "connect_nodes" => {
                let source = arg_str(name, args, "source")?;
                let target = arg_str(name, args, "target")?;
                let tab_id = arg_opt_str(args, "tab_id").map(str::to_string);
                let source_handle = arg_opt_str(args, "source_handle").map(str::to_string);
                let target_handle = arg_opt_str(args, "target_handle").map(str::to_string);
                shared(
                    name,
                    tools::connect_edge(
                        tb,
                        &self.app,
                        tab_id,
                        source,
                        target,
                        source_handle,
                        target_handle,
                    )
                    .await,
                )?
            }
            "disconnect_edge" => {
                let edge_id = arg_opt_str(args, "edge_id").map(str::to_string);
                let source = arg_opt_str(args, "source").map(str::to_string);
                let target = arg_opt_str(args, "target").map(str::to_string);
                shared(
                    name,
                    tools::disconnect_edge(tb, &self.app, edge_id, source, target).await,
                )?
            }
            _ => return Ok(None),
        };
        Ok(Some(value_to_content(out)))
    }
}

#[async_trait::async_trait]
impl ToolExecutor for NativeToolExecutor {
    fn tools(&self) -> Vec<ToolSpecDto> {
        native_tool_specs()
    }

    async fn call(&self, name: &str, arguments: Value) -> Result<String> {
        // 后端直连优先, 未命中走前端托管
        if let Some(content) = self.call_backend(name, &arguments).await? {
            return Ok(content);
        }
        if FRONTEND_TOOLS.contains(&name) {
            return self.call_frontend(name, arguments).await;
        }
        Err(tool_failed(name, "未知内置工具"))
    }
}
