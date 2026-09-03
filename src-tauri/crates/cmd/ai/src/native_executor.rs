//! 原生工具执行器 — 内置 AI 直连软件自有能力。
//!
//! 工具分两类:
//! - **后端直连**:数据读取 / 设备发送,直接调 [`mcp_server::tools`] 共享实现
//!   (与对外 MCP server 完全同一路径,零重复)。
//! - **前端托管**:节点编辑等 UI 状态操作 — 画布状态 (widgets/位置/连线/撤销)
//!   在前端 zustand store,后端经 `ai_tool_invoke` 事件桥调用前端
//!   `toolHost`,前端执行后 `ai_tool_resolve` 回执,超时兜底。
//!
//! 与外部 MCP 工具共存时内置优先 (`CompositeExecutor` 路由)。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use can_types::{CanDirection, CanFrame};
use chat::ToolExecutor;
use error::{AppError, McpError, Result};
use mcp_server::tools;
use mcp_server::Toolbox;
use parking_lot::Mutex;
use provider::ToolSpecDto;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

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

/// 构造工具失败错误。
fn tool_failed(tool: &str, details: impl Into<String>) -> AppError {
    McpError::ToolFailed {
        tool: tool.to_string(),
        details: details.into(),
    }
    .into()
}

/// 共享实现层 `Result<_, String>` → 工具失败。
fn shared<T>(tool: &str, r: std::result::Result<T, String>) -> Result<T> {
    r.map_err(|e| tool_failed(tool, e))
}

/// Value → 工具结果字符串 (对象序列化, 字符串原样)。
fn value_to_content(v: Value) -> String {
    match v {
        Value::String(s) => s,
        other => other.to_string(),
    }
}

// ============ 参数解析辅助 ============

fn arg_str<'a>(tool: &str, args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| tool_failed(tool, format!("缺少字符串参数 {key}")))
}

fn arg_opt_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn arg_f64(tool: &str, args: &Value, key: &str) -> Result<f64> {
    args.get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| tool_failed(tool, format!("缺少数值参数 {key}")))
}

#[allow(clippy::cast_possible_truncation)] // MCP 工具参数按低位截断语义 (与 JS 位运算一致)
fn arg_opt_u32(args: &Value, key: &str) -> Option<u32> {
    args.get(key).and_then(Value::as_u64).map(|v| v as u32)
}

fn arg_i64(tool: &str, args: &Value, key: &str) -> Result<i64> {
    args.get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| tool_failed(tool, format!("缺少整数参数 {key}")))
}

// MCP 工具参数按低位截断语义 (与 JS 位运算一致)
#[allow(clippy::cast_possible_truncation)]
fn arg_vec_u8(tool: &str, args: &Value, key: &str) -> Result<Vec<u8>> {
    args.get(key)
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_u64)
                .map(|v| v as u8)
                .collect()
        })
        .ok_or_else(|| tool_failed(tool, format!("缺少字节数组参数 {key}")))
}

// ============ 工具清单 ============

/// 工具规格简写构造。
fn spec(name: &str, description: &str, properties: Value, required: &[&str]) -> ToolSpecDto {
    let mut schema = json!({
        "type": "object",
        "properties": properties,
    });
    if !required.is_empty() {
        schema["required"] = json!(required);
    }
    ToolSpecDto {
        name: name.to_string(),
        description: description.to_string(),
        input_schema: schema,
    }
}

/// 内置工具清单 (静态;中文名描述)。
pub fn native_tool_specs() -> Vec<ToolSpecDto> {
    vec![
        // ---- 后端直连: 设备交互 ----
        spec(
            "list_transports",
            "列出全部传输节点 (串口/TCP/UDP/CAN 等) 及连接状态 [{node_id, state}]",
            json!({}),
            &[],
        ),
        spec(
            "list_serial_ports",
            "列出系统可用串口 [{name, port_type, vid, pid, manufacturer, product}]。连接串口前先查询端口名",
            json!({}),
            &[],
        ),
        spec(
            "send_bytes",
            "向指定传输节点发送原始字节。返回发送字节数",
            json!({
                "node_id": {"type": "string", "description": "目标传输节点 id"},
                "data": {"type": "array", "items": {"type": "integer", "minimum": 0, "maximum": 255}, "description": "字节数组"}
            }),
            &["node_id", "data"],
        ),
        spec(
            "send_string",
            "向指定传输节点发送 UTF-8 文本 (原样发送, 不自动加换行)。返回发送字节数",
            json!({
                "node_id": {"type": "string", "description": "目标传输节点 id"},
                "text": {"type": "string", "description": "要发送的文本"}
            }),
            &["node_id", "text"],
        ),
        spec(
            "send_can_frame",
            "发送 CAN 帧 (经 CAN 协议节点 encode_can 编码)",
            json!({
                "node_id": {"type": "string", "description": "目标 Transport 节点 id"},
                "protocol_node": {"type": "string", "description": "编码用 Protocol 节点 id, 缺省自动溯源"},
                "frame": {
                    "type": "object",
                    "description": "CAN 帧",
                    "properties": {
                        "id": {"type": "integer", "description": "帧 id (11/29 位)"},
                        "extended": {"type": "boolean", "description": "是否扩展帧"},
                        "data": {"type": "array", "items": {"type": "integer"}, "description": "数据字节 (最多 8)"},
                        "direction": {"type": "string", "enum": ["tx", "rx"], "description": "方向, 发送填 tx"}
                    },
                    "required": ["id", "data"]
                }
            }),
            &["node_id", "frame"],
        ),
        spec(
            "set_input_value",
            "设置节点图输入控件的值 (widget_id 为控件节点 id), 立即生效并触发求值",
            json!({
                "widget_id": {"type": "string", "description": "控件节点 id"},
                "value": {"type": "number", "description": "目标值"}
            }),
            &["widget_id", "value"],
        ),
        spec(
            "inject_bytes",
            "把字节从 source_node_id 注入全局字节平面路由到下游 (协议解析/回环), 无设备也可调试协议。返回命中下游数量",
            json!({
                "source_node_id": {"type": "string", "description": "注入源节点 id (字节边起点)"},
                "data": {"type": "array", "items": {"type": "integer", "minimum": 0, "maximum": 255}, "description": "字节数组"}
            }),
            &["source_node_id", "data"],
        ),
        // ---- 后端直连: 数据读取 ----
        spec(
            "get_graph_outputs",
            "读取节点图输出快照: {widgetId: {portId: value}} — 全部节点输出端口最新值, 观察计算/控件/波形输出的首选",
            json!({}),
            &[],
        ),
        spec(
            "get_recent_waveform",
            "读取指定数据源 (协议/FrameDecoder 节点 id) 最近 count 个采样点波形, 含通道名与数值",
            json!({
                "source": {"type": "string", "description": "数据源节点 id"},
                "count": {"type": "integer", "description": "采样点数 (上限 10000)"}
            }),
            &["source", "count"],
        ),
        spec(
            "get_waveform_window",
            "读取指定数据源时间窗口内波形 (start_ms/end_ms 为相对最新时间戳的毫秒偏移, 负数=过去)",
            json!({
                "source": {"type": "string", "description": "数据源节点 id"},
                "start_ms": {"type": "integer", "description": "窗口起点毫秒偏移, 如 -1000"},
                "end_ms": {"type": "integer", "description": "窗口终点毫秒偏移, 最新为 0"}
            }),
            &["source", "start_ms", "end_ms"],
        ),
        spec(
            "get_buffer_info",
            "读取指定数据源波形缓冲的通道数与点数 {channel_count, point_count}",
            json!({"source": {"type": "string", "description": "数据源节点 id"}}),
            &["source"],
        ),
        spec(
            "list_data_sources",
            "列出存在波形缓冲的数据源 id (配合 get_recent_waveform 使用)",
            json!({}),
            &[],
        ),
        spec(
            "get_can_frames",
            "读取最近 CAN 帧与总线负载统计 (fps, load_ratio)",
            json!({
                "count": {"type": "integer", "description": "最近帧条数 (上限 1000)"},
                "bitrate": {"type": "integer", "description": "总线比特率, 缺省 500000 (用于负载估算)"}
            }),
            &["count"],
        ),
        spec(
            "get_logic_data",
            "读取逻辑分析仪最近采样与解码事件 (UART/I2C/SPI 等)",
            json!({"count": {"type": "integer", "description": "条数 (上限 5000)"}}),
            &["count"],
        ),
        spec(
            "get_raw_data",
            "读取指定源最近收发的原始字节 (hex 编码, 分 TX/RX 方向与时间戳)。排查设备是否有数据的第一工具",
            json!({
                "source": {"type": "string", "description": "数据源节点 id (Transport 或 FrameDecoder)"},
                "max_bytes": {"type": "integer", "description": "最大读取字节 (上限 64KiB)"}
            }),
            &["source"],
        ),
        // ---- 知识库 ----
        spec(
            "read_skill",
            "读取内置知识库文档全文 (id 见系统提示词索引)",
            json!({
                "skill_id": {"type": "string", "description": "文档 id, 如 overview / nodes-reference / protocols / debug-recipes / tools-guide"},
                "lang": {"type": "string", "enum": ["zh", "en"], "description": "文档语言, 缺省 zh"}
            }),
            &["skill_id"],
        ),
        // ---- 前端托管: 节点编辑与 UI 操作 ----
        spec(
            "get_workspace",
            "读取画布全量状态: tabs、活跃 tab、全部 widget (id/kind/位置/配置/端口表)、全局 transport/protocol 节点 (id/配置/端口表)、全部连线。编辑节点前必读; 连线前对照各节点 ports 的 domain (同域才可连)",
            json!({}),
            &[],
        ),
        spec(
            "add_node",
            "添加节点: transport (传输) / protocol (协议) / widget (控件)。返回新节点 id",
            json!({
                "type": {"type": "string", "enum": ["transport", "protocol", "widget"], "description": "节点类别"},
                "kind": {"type": "string", "description": "类型: transport=Serial/Udp/TcpClient/TcpServer/TestData/Slcan/CandleLight; protocol=JustFloat/FireWater/RawData/Slcan/CandleLight/LogicDecode; widget=Knob/Slider/Button/Waveform/Math/FrameDecoder/..."},
                "tab_id": {"type": "string", "description": "widget 归属 tab, 缺省当前活跃 tab"},
                "config": {"type": "object", "description": "配置 (可选, 与默认配置深合并)"},
                "position": {"type": "object", "description": "画布位置 {x, y}, 缺省自动排布", "properties": {"x": {"type": "number"}, "y": {"type": "number"}}}
            }),
            &["type", "kind"],
        ),
        spec(
            "update_node_config",
            "更新节点配置 (widget 为 params 深合并; transport/protocol: kind 可变 — kind 变化时配置整体替换, 其余字段深合并/重建)",
            json!({
                "node_id": {"type": "string", "description": "目标节点 id"},
                "config": {"type": "object", "description": "新配置 (部分或完整)"}
            }),
            &["node_id", "config"],
        ),
        spec(
            "remove_node",
            "删除节点 (widget 或全局 transport/protocol), 自动清理其连线并关闭连接",
            json!({"node_id": {"type": "string", "description": "目标节点 id"}}),
            &["node_id"],
        ),
        spec(
            "connect_nodes",
            "连接两个节点的端口 (后端编译校验)。handle 缺省时自动补默认端口;RawData 控件目标自动改写 src: 端口。端口域不匹配 (如频域接时域) 或成环会直接报错且不建边 — 错误信息含真实原因, 换端口或改配置后重试。成功返回 edge_id 并实时同步画布",
            json!({
                "source": {"type": "string", "description": "源节点 id"},
                "source_handle": {"type": "string", "description": "源端口 id (如 rx / out / ch0), 缺省自动"},
                "target": {"type": "string", "description": "目标节点 id"},
                "target_handle": {"type": "string", "description": "目标端口 id (如 in / data), 缺省自动"},
                "tab_id": {"type": "string", "description": "归属 tab, 缺省自动定位 (优先同时持有两端的 tab)"}
            }),
            &["source", "target"],
        ),
        spec(
            "disconnect_edge",
            "删除连线: 给 edge_id 精确删除, 或给 source+target (可只给一端) 删除第一条匹配",
            json!({
                "edge_id": {"type": "string", "description": "连线 id"},
                "source": {"type": "string", "description": "源节点 id (与 target 组合过滤)"},
                "target": {"type": "string", "description": "目标节点 id"}
            }),
            &[],
        ),
        spec(
            "move_node",
            "移动节点画布位置 (纯视觉调整)",
            json!({
                "node_id": {"type": "string", "description": "目标节点 id"},
                "x": {"type": "number"}, "y": {"type": "number"}
            }),
            &["node_id", "x", "y"],
        ),
        spec(
            "create_tab",
            "新建控制页 (画布 tab)。返回 tab_id",
            json!({"name": {"type": "string", "description": "页名, 缺省自动编号"}}),
            &[],
        ),
        spec(
            "set_active_tab",
            "切换活跃控制页",
            json!({"tab_id": {"type": "string", "description": "目标 tab id"}}),
            &["tab_id"],
        ),
        spec(
            "connect_transport",
            "打开传输连接 (串口/TCP/UDP/CAN/TestData)。连接后即可收发数据",
            json!({"node_id": {"type": "string", "description": "传输节点 id"}}),
            &["node_id"],
        ),
        spec(
            "disconnect_transport",
            "关闭传输连接",
            json!({"node_id": {"type": "string", "description": "传输节点 id"}}),
            &["node_id"],
        ),
        spec(
            "list_templates",
            "列出内置工作区模板 (id 与说明), 配合 apply_template 使用",
            json!({}),
            &[],
        ),
        spec(
            "apply_template",
            "一键应用内置工作区模板 (自动搭建传输→协议→显示链路)",
            json!({"template_id": {"type": "string", "description": "模板 id (先 list_templates 查询)"}}),
            &["template_id"],
        ),
    ]
}

/// 前端托管工具名集合。
const FRONTEND_TOOLS: &[&str] = &[
    "get_workspace",
    "add_node",
    "update_node_config",
    "remove_node",
    "move_node",
    "create_tab",
    "set_active_tab",
    "connect_transport",
    "disconnect_transport",
    "list_templates",
    "apply_template",
];

/// 后端直连工具名集合 (与 `call_backend` 的 match 分支一一对应)。
const BACKEND_TOOLS: &[&str] = &[
    "list_transports",
    "list_serial_ports",
    "send_bytes",
    "send_string",
    "send_can_frame",
    "set_input_value",
    "inject_bytes",
    "get_graph_outputs",
    "get_recent_waveform",
    "get_waveform_window",
    "get_buffer_info",
    "list_data_sources",
    "get_can_frames",
    "get_logic_data",
    "get_raw_data",
    "read_skill",
    // 连线拓扑 — 后端权威 (编译校验 + graph:source 事件同步画布)
    "connect_nodes",
    "disconnect_edge",
];

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

/// 解析 CAN 帧参数。
#[allow(clippy::cast_possible_truncation)] // CAN id 超范围按低位截断 (与工具层约定一致)
fn parse_can_frame(tool: &str, args: &Value) -> Result<CanFrame> {
    let frame = args
        .get("frame")
        .ok_or_else(|| tool_failed(tool, "缺少 frame 参数"))?;
    let id = frame
        .get("id")
        .and_then(Value::as_u64)
        .ok_or_else(|| tool_failed(tool, "frame.id 缺失"))? as u32;
    let data = arg_vec_u8(tool, frame, "data")?;
    let extended = frame
        .get("extended")
        .and_then(Value::as_bool)
        .unwrap_or(id > 0x7FF);
    let direction = match frame.get("direction").and_then(Value::as_str) {
        Some("tx" | "Tx" | "TX") => CanDirection::Tx,
        _ => CanDirection::Rx,
    };
    Ok(CanFrame {
        timestamp: vofa_core::now_us(),
        id,
        extended,
        rtr: frame.get("rtr").and_then(Value::as_bool).unwrap_or(false),
        dlc: u8::try_from(data.len().min(8)).unwrap_or(8),
        data: data.into_iter().take(8).collect(),
        direction,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 内置清单完备: specs 与两路名字集合一致, 无重名, 前端工具都有声明。
    #[test]
    fn specs_cover_all_tools_without_duplicates() {
        let specs = native_tool_specs();
        let mut seen = std::collections::HashSet::new();
        for t in &specs {
            assert!(seen.insert(t.name.as_str()), "重复工具名: {}", t.name);
            assert!(!t.description.is_empty());
            assert_eq!(t.input_schema["type"], "object");
        }
        assert_eq!(seen.len(), BACKEND_TOOLS.len() + FRONTEND_TOOLS.len());
        for f in FRONTEND_TOOLS {
            assert!(seen.contains(f), "前端工具 {f} 未在 specs 中声明");
        }
        for b in BACKEND_TOOLS {
            assert!(seen.contains(b), "后端工具 {b} 未在 specs 中声明");
        }
        assert!(specs.len() >= 25);
    }

    /// handles 命中内置名, 不命中外部风格名。
    #[test]
    fn handles_routes_by_name() {
        assert!(NativeToolExecutor::handles("get_workspace"));
        assert!(NativeToolExecutor::handles("send_string"));
        assert!(NativeToolExecutor::handles("read_skill"));
        assert!(!NativeToolExecutor::handles("mcp_foo_bar"));
        assert!(!NativeToolExecutor::handles("send_string_extra"));
    }

    /// 参数解析: 缺参报错, 数组截断/类型收窄正确。
    #[test]
    fn arg_helpers_validate() {
        let args = json!({"node_id": "transport-1", "data": [1, 300, 7]});
        assert_eq!(arg_str("t", &args, "node_id").unwrap(), "transport-1");
        let bytes = arg_vec_u8("t", &args, "data").unwrap();
        assert_eq!(bytes, vec![1, 44, 7]); // 300 截断为 u8
        assert!(arg_str("t", &args, "missing").is_err());
        assert!(arg_vec_u8("t", &args, "node_id").is_err()); // 类型不匹配
    }

    /// CAN 帧解析: id/extended 推断/方向/8 字节截断。
    #[test]
    fn can_frame_parses() {
        let args =
            json!({"frame": {"id": 0x123, "data": [9, 8, 7, 6, 5, 4, 3, 2, 1], "direction": "tx"}});
        let f = parse_can_frame("t", &args).unwrap();
        assert_eq!(f.id, 0x123);
        assert!(!f.extended);
        assert_eq!(f.direction, CanDirection::Tx);
        assert_eq!(f.data.len(), 8);
        assert_eq!(f.dlc, 8);

        let ext = json!({"frame": {"id": 0x1ABCDEF0, "data": [1]}});
        let f = parse_can_frame("t", &ext).unwrap();
        assert!(f.extended, "29 位 id 应推断为扩展帧");
        assert_eq!(f.direction, CanDirection::Rx);
    }

    /// 工具结果字符串化: 字符串去引号, 对象序列化。
    #[test]
    fn value_content_roundtrip() {
        assert_eq!(value_to_content(Value::String("ok".into())), "ok");
        assert_eq!(value_to_content(json!({"a": 1})), r#"{"a":1}"#);
    }
}
