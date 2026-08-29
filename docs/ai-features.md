# AI 功能架构 (AI 对话 + MCP 双向 + 内置工具层)

VOFA-NEXT 的 AI 能力统一规划在 Rust 后端,前端只是薄 UI。三条通道:

1. **AI 对话**(出站):前端聊天面板 → Tauri 命令 → 后端多 provider 聚合层调用 LLM,
   流式增量经 `Channel` 推回;模型可调用工具(工具调用循环)——工具来自
   **内置原生工具层**(软件自有能力)与 **外部 MCP server** 两组,独立开关。
2. **MCP server**(入站):后端在 `127.0.0.1:{port}/mcp` 起一个 streamable-http MCP 服务,
   把本应用能力(串口发送、波形读取、节点图编辑等)暴露为 **MCP 工具**,
   外部 AI 客户端(Claude Desktop / ZCode 等)可直接控制本应用。
3. **内置知识库(skills)**:随应用打包的软件使用文档(zh/en),系统提示词注入
   索引,`read_skill` 工具按需读全文。

```
前端 AiChatPanel(薄视图) ──Tauri IPC──▶ cmd_ai ──▶ ai_chat(工具调用循环)──▶ ai_provider(genai 封装)
                                 │              │        │
                                 │              │        ▼
                                 │              │     ai_session(多会话 + 历史持久化, 后端持有)
                                 │              ▼
                                 ├──▶ mcp_server(VOFA 能力 → MCP 工具, 127.0.0.1)
                                 │        └─ tools(共享工具实现, 内置执行器复用)
                                 ├──▶ mcp_client(连接外部 MCP server, 聚合工具给对话)
                                 └──▶ native_executor(内置原生工具) ──事件桥──▶ 前端 toolHost(节点编辑)
```

## 后端 crate(全部单一职责)

| crate | 职责 | 关键类型/函数 |
|---|---|---|
| `ai_provider` | LLM provider 聚合,封装 `genai 0.6` | `build_client`(AuthResolver 注 key / ServiceTargetResolver 覆盖端点)、`chat_turn_stream`(流式归一化)、`validate_config`、`ADAPTERS`(含 `orcarouter`) |
| `ai_chat` | 多轮工具调用循环 + 任务取消 | `run_chat`、`TurnProvider` / `ToolExecutor`(均可 mock,循环逻辑离线单测)、`ChatTaskRegistry`(watch 取消)、`TurnRecorder`(流式事件 → 可持久化条目) |
| `ai_session` | 会话持久化(所有权在后端) | `SessionStore`(多会话 CRUD + 落盘 `ai_chat_sessions.json`)、`ViewItemDto`/`ChatSession`(对齐前端视图)、`derive_history`(条目流 → LLM 消息) |
| `mcp_client` | 连接外部 MCP server(stdio 子进程 / streamable-http) | `McpManager`(连接池、工具聚合加前缀 `mcp_{server}_{tool}`、路由调用)、配置持久化 `mcp_servers.json` |
| `mcp_server` | 把本应用能力暴露为 MCP 工具 | `Toolbox`(AppState 共享句柄切片)、`VofaMcpServer`(rmcp `#[tool_router]`)、`tools`(共享工具实现,内置执行器直接复用)、`start` |
| `cmd_ai` | Tauri 命令层 + 内置原生工具 + 知识库 | `AiState`(managed:任务表 / 会话存储 / 连接管理器 / 工具缓存 / server 句柄 / 前端托管调用注册表)、`ai_chat_send`(Channel 流式 + 双工具源组合)、`ai_tool_resolve`(前端回执)、`native_executor`(内置工具执行器)、`skills`(知识库文档 + 系统提示词组装)、`chat_*` 会话命令 |

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
工具实现统一在 `mcp_server::tools`(普通异步函数,`Result<Value, String>`),
rmcp handler 只做参数包装——内置原生工具执行器调用同一批函数,零重复。

| 工具 | 能力 |
|---|---|
| `list_transports` / `list_serial_ports` | 传输节点与连接状态 / 系统可用串口 |
| `send_bytes` / `send_string` / `send_can_frame` | 向设备发送字节 / UTF-8 文本 / CAN 帧 |
| `inject_bytes` | 字节注入(沿全局字节平面路由,喂协议 / FrameDecoder / 回环) |
| `set_input_value` | 设置节点图输入控件值 |
| `get_graph_outputs` | 读取节点图输出快照 |
| `get_recent_waveform` / `get_waveform_window` / `get_buffer_info` / `list_data_sources` | 最近波形 / 时间窗波形 / 缓冲信息 / 数据源清单 |
| `get_can_frames` / `get_logic_data` / `get_raw_data` | 最近 CAN 帧 + 负载 / 逻辑采样与解码事件 / 原始字节(TX/RX, hex) |
| `list_tabs` / `update_graph` | 列出图 tab / 提交替换节点图(复用 `apply_tab_graph_parts`;可选 `widgets` 配置记录 + `positions` 位置,画布可完整渲染控件) |
| `connect_edge` / `disconnect_edge` | 增量连线 / 删线(后端编译校验,失败返回真实原因且不建边) |

外部客户端接入示例(Claude Desktop connectors / 任意 MCP 客户端):

```json
{ "url": "http://127.0.0.1:8765/mcp", "transport": "streamable-http" }
```

## 内置原生工具层(内置 AI 直连软件能力)

`cmd_ai::native_executor::NativeToolExecutor` 实现 `ai_chat::ToolExecutor`,
与外部 MCP 工具经 `CompositeExecutor` 组合(同名内置优先;两组独立开关
`builtinToolsEnabled` / `mcpToolsEnabled`)。工具分两类执行路径:

- **后端直连**(数据读取 / 设备发送 / 知识库 / **连线拓扑**):直接调用
  `mcp_server::tools` 共享函数,与对外 MCP server 同一路径。包括 `list_transports`、
  `send_bytes` / `send_string` / `send_can_frame`、`set_input_value`、
  `inject_bytes`、`get_graph_outputs`、`get_recent_waveform` /
  `get_waveform_window` / `get_buffer_info` / `list_data_sources`、
  `get_can_frames` / `get_logic_data` / `get_raw_data`、`list_serial_ports`、
  `read_skill`,以及连线拓扑 `connect_nodes` / `disconnect_edge`(后端权威,
  见下节)。读取类工具设上限(波形 ≤10000 点、CAN ≤1000 帧、逻辑 ≤5000
  条、原始字节 ≤64KiB)防超长返回。
- **前端托管**(节点编辑 / UI 操作):画布 UI 态(widgets 配置、位置、撤销
  历史)在前端 zustand store,后端无法直接变更。执行器经事件桥调用前端:
  emit `ai_tool_invoke {call_id, name, arguments}` → 前端 `toolHost.ts`
  分发 handler(全部走 `useAppStore` 现有 action:`addWidget` /
  `setTransportNodeConfig` …,画布实时刷新、撤销历史可用、tab 图自动同步
  后端)→ `ai_tool_resolve {call_id, ok, result}` 回执;15s 超时兜底。包括
  `get_workspace`(读画布全量状态,含各节点端口表与 domain,编辑前必读)、
  `add_node` / `update_node_config` / `remove_node` / `move_node`、
  `create_tab` / `set_active_tab`、`connect_transport` / `disconnect_transport`、
  `list_templates` / `apply_template`(默认 merge 模式,不破坏用户现有工作区)。

### 节点图与 widget 配置:后端权威

节点图的全部状态 — 连线拓扑 (NodeDef/Edge)、widget 配置记录 (kind + params)、
画布位置、tab 元数据 — 所有权都在后端,前端画布是权威状态的投影:

- **源图存储** (`AppState.source_graphs`,每 tab 的 NodeDef + Edge + 端口提示 +
  widget 配置记录):整图提交 (前端 sync / MCP `update_graph`) 与拓扑 op
  (`connect_nodes` / `connect_edge` / `disconnect_edge`) 共用
  `apply_tab_graph_parts` 同一编译提交入口;
- **工作区存储** (`AppState.workspace`):控件 tab 清单 (id/名称/widget 顺序)、
  数据面板 tab 元数据、全部节点画布位置,随图提交在同一事务里更新
  (编译失败则一切不变);
- **编译失败 (端口域不匹配 / 成环) 返回真实 `CompileError` 且源图不变** ——
  不再回退占位 Cycle 假错误;错误边结构上不可能存在,模型收到错误后可自我修正;
- 默认 handle 与 RawData `src:` 端口改写依据前端 sync 附带的端口提示
  (`SourceNodeHint` — widget 端口表形状由前端参数派生,后端经提示解析);
- 提交成功后 emit **`graph:source` {tab_id, version, nodes, edges, widgets,
  positions}**,前端画布按此收敛 (`adoptSourceGraph`:边替换为权威集、补建缺失
  全局节点、widget 节点集合收敛到配置记录 — 外部提交的**纯 widget 图可完整
  渲染**,外部删除同样生效);
- **画布是纯投影, 前端不做连线有效性判断** — 后端编译认可的连线画布逐字
  保留, 不存在"前端认为悬空就删除"的路径;未知 widget kind 的记录落为
  占位控件 (通用卡片 + 默认端口) 而非丢弃;本 tab 提交在途期间的 graph:source
  自回声暂缓采纳, 由提交响应自身收敛;
- 前端整图提交携带 `base_version` 乐观并发基线,版本被其他写入方推进时后端
  返回 `GraphVersionConflict`,前端拉取权威源图合并后重试一次(多写入方防互踩);
- `update_tab_graph` 成功响应与 `graph:derived` 事件携带新版本号;
- 拖拽结束 (`dragging=false` 收尾批) 经 `set_node_positions` 轻量上报最终位置
  (不触发编译);
- widget 配置记录的 schema 语义仍由前端类型定义 (后端以 `Value` 透传,
  不复刻 28 类控件 schema),未知 kind 在画布水合/采纳时剔除 —— 后端是
  **存储与分发权威**,配置形状校验发生在前端。

外部 MCP 客户端的 `update_graph`(可选 `widgets` / `positions` 参数)、
`connect_edge` / `disconnect_edge` 走同一后端入口,画布经 `graph:source`
实时同步。

## 工作区持久化 (workspace.json)

工作区整体持久化在 app config dir 的 `workspace.json`(形态与
`ai_chat_sessions.json` / `mcp_servers.json` 一致):控件 tab + 数据面板元数据、
各 tab 源图 (NodeDef/Edge/端口提示/widget 记录) 与画布位置。

- **写入**:图提交 / 位置上报 / tab 元数据变更置 dirty,后台任务 800ms 防抖
  整体覆盖写;应用退出时 flush 一次,不丢最后一次编辑;
- **启动恢复**:`lib.rs` setup 阶段加载文件并逐 tab 重编译 (不发事件 —
  前端尚未就绪),图在后端立即可求值;单 tab 恢复失败不阻塞整体启动;
- **前端水合**:启动时 `workspace_get` 拉取权威快照 (含全局版本号作为
  base_version 基线) 覆盖本地;无持久化工作区 (全新安装) 返回 None,前端走
  默认启动 (初始同步 + 种子图);
- tab 元数据 (改名/增删/重排) 由前端订阅 store 变化经 `workspace_set_tabs`
  整表覆盖。

## 内置知识库(skills)

`cmd_ai::skills` + `crates/cmd_ai/skills/{zh,en}/*.md`(`include_str!` 编译期
嵌入):`overview`(软件与核心概念)、`nodes-reference`(节点/控件参考)、
`protocols`(协议格式)、`debug-recipes`(调试实战)、`tools-guide`(工具指南)。

- 启用内置工具时,`ai_chat_send` 把系统提示词组装为:基础工作约定(按
  `ui_lang` 选语言)+ 知识库索引(id + 标题 + 一句话用途)+ 用户自填提示词;
- 模型按需 `read_skill {skill_id, lang?}` 读全文,避免把整本文档塞进上下文;
- 文档为 zh / en 双份,跟随界面语言注入。

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
  systemPrompt / maxToolRounds / builtinToolsEnabled / mcpToolsEnabled / mcpServerPort)
- `src/lib/ai/toolHost.ts`:内置 AI 前端托管工具宿主 — 监听 `ai_tool_invoke`,
  经 `useAppStore` 现有 action 执行节点编辑 / UI 操作,`ai_tool_resolve` 回执;
  App 挂载时 `initAiToolHost()`(幂等)

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
- 前端托管工具依赖 webview 存活(聊天面板本身就在其中,实际恒成立),
  15s 超时兜底;webview 处于后台且被系统挂起时可能超时
- 后端以 `Value` 透传 widget params, schema 语义由前端定义:外部写入方提交
  未知 kind 的记录时,该控件在画布上落为占位卡片(渲染不出控件本体),
  写入方需参照 `get_workspace` 返回的 config 形态构造 params
- 工作区持久化不含窗口停靠布局(localStorage)与应用设置(settings.json);
  传输连接状态属运行态,重启后不自动重连
