# AI 功能架构 (AI 对话 + MCP 双向)

VOFA-NEXT 的 AI 能力统一规划在 Rust 后端,前端只是薄 UI。两条通道:

1. **AI 对话**(出站):前端聊天面板 → Tauri 命令 → 后端多 provider 聚合层调用 LLM,
   流式增量经 `Channel` 推回;模型可调用 **外部 MCP server** 的工具(工具调用循环)。
2. **MCP server**(入站):后端在 `127.0.0.1:{port}/mcp` 起一个 streamable-http MCP 服务,
   把本应用能力(串口发送、波形读取、节点图编辑等)暴露为 **MCP 工具**,
   外部 AI 客户端(Claude Desktop / ZCode 等)可直接控制本应用。

```
前端 AiChatPanel(薄视图) ──Tauri IPC──▶ cmd_ai ──▶ ai_chat(工具调用循环)──▶ ai_provider(genai 封装)
                                 │              │        │
                                 │              │        ▼
                                 │              │     ai_session(多会话 + 历史持久化, 后端持有)
                                 │              ▼
                                 ├──▶ mcp_server(VOFA 能力 → MCP 工具, 127.0.0.1)
                                 └──▶ mcp_client(连接外部 MCP server, 聚合工具给对话)
```

## 后端 crate(全部单一职责)

| crate | 职责 | 关键类型/函数 |
|---|---|---|
| `ai_provider` | LLM provider 聚合,封装 `genai 0.6` | `build_client`(AuthResolver 注 key / ServiceTargetResolver 覆盖端点)、`chat_turn_stream`(流式归一化)、`validate_config`、`ADAPTERS`(含 `orcarouter`) |
| `ai_chat` | 多轮工具调用循环 + 任务取消 | `run_chat`、`TurnProvider` / `ToolExecutor`(均可 mock,循环逻辑离线单测)、`ChatTaskRegistry`(watch 取消)、`TurnRecorder`(流式事件 → 可持久化条目) |
| `ai_session` | 会话持久化(所有权在后端) | `SessionStore`(多会话 CRUD + 落盘 `ai_chat_sessions.json`)、`ViewItemDto`/`ChatSession`(对齐前端视图)、`derive_history`(条目流 → LLM 消息) |
| `mcp_client` | 连接外部 MCP server(stdio 子进程 / streamable-http) | `McpManager`(连接池、工具聚合加前缀 `mcp_{server}_{tool}`、路由调用)、配置持久化 `mcp_servers.json` |
| `mcp_server` | 把本应用能力暴露为 MCP 工具 | `Toolbox`(AppState 共享句柄切片)、`VofaMcpServer`(rmcp `#[tool_router]`)、`start` |
| `cmd_ai` | Tauri 命令层 | `AiState`(managed:任务表 / 会话存储 / 连接管理器 / 工具缓存 / server 句柄)、`ai_chat_send`(Channel 流式)、`chat_*` 会话命令 |

依赖理由(遵循 AGENTS.md):

- **genai 0.6**:26+ LLM provider 原生协议开箱(OpenAI / Anthropic / Gemini / DeepSeek /
  通义 / Kimi / GLM / Ollama / OpenRouter 等),流式与工具调用完备;
  内部即 reqwest+rustls,与仓库既有 HTTP 栈一致,避免手写各 provider 协议。
- **rmcp 3.1**:Model Context Protocol 官方 Rust SDK,client/server 双向 +
  stdio / streamable-http 传输,MCP 协议不在本仓库自研。
- **axum 0.8**:rmcp streamable-http server 传输是 tower service,需要 HTTP 宿主挂载。
- **react-markdown + remark-gfm + rehype-highlight**(前端):AI 回复 Markdown 渲染。
  组件化输出、不注入原始 HTML(XSS 安全);自研解析既不安全工作量也大,
  `rehype-highlight`(highlight.js)补齐代码块高亮。
- **keyring 3**:系统凭据库(macOS Keychain / Windows Credential Manager / libsecret)。
  AI provider 的 API key 存钥匙串而非明文 settings.json;自研加密落盘仍可被提取,
  系统凭据库才是正确方案。

### 对话事件契约(`Channel<AiChatEvent>`,`tag = "type"`)

`delta` / `reasoning_delta`(增量)→ `tool_call` / `tool_result`(工具回合)→
`done` / `cancelled` / `error`(终止)。前端 `src/types/ai.ts` 严格对齐。

`error` 事件携带 `kind`(`AppError::kind()`,如 `AiProviderRequest`)与 `data`
(adapter / model / rounds 等结构化字段),前端 `src/lib/ai/aiErrors.ts` 按 kind
本地化展示,原始描述降级为次要信息;错误条目持久化时同样保留 `error_kind` /
`error_data`。命令级失败(IPC reject)为同一形态的 `{ kind, message, data }`。

### 会话与历史(后端持有)

历史不再由前端携带:会话以"视图条目流"形式持久化在 app config dir 的
`ai_chat_sessions.json`(`ai_session` crate,形态与 `mcp_servers.json` 一致)。

- `ai_chat_send(session_id, text, regenerate, ...)`:发送时后端先落盘用户条目
  (或 `regenerate` 时截掉最后一条用户条目之后的回合),`derive_history` 派生
  LLM 上下文;回合终态后 `TurnRecorder` 聚合出的助手条目(文本 + 工具卡片 +
  错误)落盘,前端从 `chat_get_session` 拉取权威视图对账。
- 会话命令:`chat_list_sessions` / `chat_create_session` / `chat_get_session` /
  `chat_rename_session` / `chat_delete_session` / `chat_clear_session`。
- `error` 条目与未完成的工具调用只用于 UI 展示,不入 LLM 历史
  (未完成的调用在收束时标记失败,保证 `tool_calls` 与结果配对)。

### 取消语义

每次 `ai_chat_send` 返回 `task_id`;`ai_chat_cancel` 置位 watch 标志,
循环在流读取点(每条流事件)与工具执行前检查,延迟为一条流事件的间隔。

## Provider:OrcaRouter(默认适配器)

设置 → AI 的默认 provider 为 **OrcaRouter**(`https://api.orcarouter.ai/v1`,
OpenAI 兼容聚合网关,可调目录内任意厂商模型,含 Anthropic / Gemini):

- 模型名需带厂商前缀:`openai/gpt-4o-mini`、`anthropic/claude-sonnet-4`;
- base_url 留空即走官方端点(后端 `ai_provider::ORCAROUTER_ENDPOINT` 兜底),
  也可自定义网关地址;
- 通过[推广链接](https://www.orcarouter.ai/ref/ref_1f7582998bdadbe7e0f3)
  注册可获取 API Key(推广码 `ref_1f7582998bdadbe7e0f3`,支持本项目)。

其余 provider(openai / anthropic / gemini / deepseek / 通义 / Kimi / GLM /
Ollama / OpenRouter 等)照旧,完整清单见设置下拉(与 `ai_provider::ADAPTERS` 一致)。

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

- `src/components/ai/AiChatPanel.tsx`:可停靠对话面板 — 默认右侧,标题栏可拖拽
  重新停靠到 左/右/下,空白处松手浮动(右下角把手调尺寸);标题栏含会话下拉
  (新建 / 重命名 / 删除)与 MCP server 抽屉
- `src/components/ai/AiMarkdown.tsx`:AI 回复 Markdown 渲染
  (GFM + 代码高亮 + 代码块/消息复制;user 消息仍为纯文本)
- `src/store/aiChatStore.ts`:薄视图层 — 会话列表 / 乐观流式聚合 / 工具记录 /
  本地 server 管理;历史由后端持有,终态后从 `chat_get_session` 对账
- `src/store/layoutStore.ts`:AI 面板布局(`aiPanelVisible` / `aiDock`
  right|left|bottom|float / `aiFloatRect`,localStorage 持久化)与侧边栏停靠
- `src/lib/dockDrag.ts`:指针拖拽控制器(AI 面板为 `ai-panel` 拖拽源 +
  `ai-dock` 边缘热区,复用侧边栏同款机制)
- `src/settings/defaults.ts` + `src/components/settingFields.ts`:设置 `ai` 分类
  (adapter 默认 `orcarouter` / baseUrl / apiKey / model / temperature / maxTokens /
  systemPrompt / maxToolRounds / mcpToolsEnabled / mcpServerPort)

## API key 存储

密钥经 `ai_keychain_*` 命令存系统钥匙串(`service = "vofa-next"`,
`user = "ai-api-key-{adapter}"`,按适配器隔离);settings.json 与配置备份文件中
恒为空串,启动时从钥匙串水合,旧版本明文自动迁移。发送前前端按后端
`validate_config` 同规则预检(`src/settings/aiProvider.ts`),配置缺失在面板
内联提示并禁用发送。

## 已知限制(后续拓展方向)

- 工具入参 schema 在对话侧未做校验(交由 provider 与 server)
- 对话历史无条数上限策略(全量 JSON 落盘,会话极多时可考虑分文件 / 截断策略)
- 流式中切换会话后,流式气泡不在新会话内显示(回合仍写入发起它的会话)
