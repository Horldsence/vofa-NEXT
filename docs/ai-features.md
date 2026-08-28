# AI 功能架构 (AI 对话 + MCP 双向)

VOFA-NEXT 的 AI 能力统一规划在 Rust 后端,前端只是薄 UI。两条通道:

1. **AI 对话**(出站):前端聊天面板 → Tauri 命令 → 后端多 provider 聚合层调用 LLM,
   流式增量经 `Channel` 推回;模型可调用 **外部 MCP server** 的工具(工具调用循环)。
2. **MCP server**(入站):后端在 `127.0.0.1:{port}/mcp` 起一个 streamable-http MCP 服务,
   把本应用能力(串口发送、波形读取、节点图编辑等)暴露为 **MCP 工具**,
   外部 AI 客户端(Claude Desktop / ZCode 等)可直接控制本应用。

```
前端 AiChatPanel ──Tauri IPC──▶ cmd_ai ──▶ ai_chat(工具调用循环)──▶ ai_provider(genai 封装)
                                 │                │
                                 │                ▼
                                 ├──▶ mcp_server(VOFA 能力 → MCP 工具, 127.0.0.1)
                                 └──▶ mcp_client(连接外部 MCP server, 聚合工具给对话)
```

## 后端 crate(全部单一职责)

| crate | 职责 | 关键类型/函数 |
|---|---|---|
| `ai_provider` | LLM provider 聚合,封装 `genai 0.6` | `build_client`(AuthResolver 注 key / ServiceTargetResolver 覆盖端点)、`chat_turn_stream`(流式归一化)、`validate_config` |
| `ai_chat` | 多轮工具调用循环 + 任务取消 | `run_chat`、`TurnProvider` / `ToolExecutor`(均可 mock,循环逻辑离线单测)、`ChatTaskRegistry`(watch 取消) |
| `mcp_client` | 连接外部 MCP server(stdio 子进程 / streamable-http) | `McpManager`(连接池、工具聚合加前缀 `mcp_{server}_{tool}`、路由调用)、配置持久化 `mcp_servers.json` |
| `mcp_server` | 把本应用能力暴露为 MCP 工具 | `Toolbox`(AppState 共享句柄切片)、`VofaMcpServer`(rmcp `#[tool_router]`)、`start` |
| `cmd_ai` | Tauri 命令层 | `AiState`(managed:任务表 / 连接管理器 / 工具缓存 / server 句柄)、`ai_chat_send`(Channel 流式) |

依赖理由(遵循 AGENTS.md):

- **genai 0.6**:26+ LLM provider 原生协议开箱(OpenAI / Anthropic / Gemini / DeepSeek /
  通义 / Kimi / GLM / Ollama / OpenRouter 等),流式与工具调用完备;
  内部即 reqwest+rustls,与仓库既有 HTTP 栈一致,避免手写各 provider 协议。
- **rmcp 3.1**:Model Context Protocol 官方 Rust SDK,client/server 双向 +
  stdio / streamable-http 传输,MCP 协议不在本仓库自研。
- **axum 0.8**:rmcp streamable-http server 传输是 tower service,需要 HTTP 宿主挂载。

### 对话事件契约(`Channel<AiChatEvent>`,`tag = "type"`)

`delta` / `reasoning_delta`(增量)→ `tool_call` / `tool_result`(工具回合)→
`done` / `cancelled` / `error`(终止)。前端 `src/types/ai.ts` 严格对齐。

### 取消语义

每次 `ai_chat_send` 返回 `task_id`;`ai_chat_cancel` 置位 watch 标志,
循环在流读取点(每条流事件)与工具执行前检查,延迟为一条流事件的间隔。

## MCP server 工具清单(本应用能力)

默认 `http://127.0.0.1:8765/mcp`(端口可在设置 → AI 修改;仅监听回环地址)。

| 工具 | 能力 |
|---|---|
| `list_transports` | 传输节点与连接状态 |
| `send_bytes` / `send_string` | 向设备发送字节 / UTF-8 文本 |
| `inject_bytes` | 字节注入(沿全局字节平面路由,喂协议 / FrameDecoder / 回环) |
| `set_input_value` | 设置节点图输入控件值 |
| `get_graph_outputs` | 读取节点图输出快照 |
| `get_recent_waveform` / `list_data_sources` | 读取最近波形窗口 / 列出数据源 |
| `list_tabs` / `update_graph` | 列出图 tab / 提交替换节点图(复用 `apply_tab_graph_parts`,前端界面实时同步) |

外部客户端接入示例(Claude Desktop connectors / 任意 MCP 客户端):

```json
{ "url": "http://127.0.0.1:8765/mcp", "transport": "streamable-http" }
```

## 外部 MCP server 接入(供 AI 对话调用)

设置 → AI 中无需配置;在聊天面板 → MCP 服务器 抽屉中添加:

- **stdio**:`command` + `args`(如 `npx -y @modelcontextprotocol/server-filesystem`)
- **http**:`url`(如 `http://host:8000/mcp`)

配置持久化于 app config dir 的 `mcp_servers.json`。`mcp_list_tools` 会自动连接
已启用的 server 并聚合工具(前缀 `mcp_{server}_{tool}` 防重名);对话时
`mcpToolsEnabled` 开启即把缓存快照中的工具提供给模型。

## 前端结构

- `src/components/panels/bottom/AiChatPanel.tsx`:底部对话面板
  (状态栏 ✦ 按钮开关;面板关闭不丢会话)
- `src/store/aiChatStore.ts`:会话视图 / 流式聚合 / 工具记录 / 本地 server 管理
- `src/settings/defaults.ts` + `src/components/settingFields.ts`:设置 `ai` 分类
  (adapter / baseUrl / apiKey / model / temperature / maxTokens / systemPrompt /
  maxToolRounds / mcpToolsEnabled / mcpServerPort)

## 已知限制(v1,后续拓展方向)

- API key 明文存于 settings.json(未接系统 keychain)
- 对话历史仅在内存(重启丢失)
- 工具入参 schema 在对话侧未做校验(交由 provider 与 server)
