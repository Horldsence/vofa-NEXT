//! AI 功能类型 — 与后端 `ai_provider` / `ai_chat` / `mcp_client` 的 serde
//! 结构严格对应 (snake_case 字段逐一对应, 勿改名)。

/// 对话消息角色。
export type AiChatRole = 'system' | 'user' | 'assistant' | 'tool';

/// 工具调用描述 (assistant 消息携带)。
export interface AiToolCall {
  id: string;
  name: string;
  arguments: unknown;
}

/// 对话消息 (发送完整历史给后端)。
export interface AiChatMessage {
  role: AiChatRole;
  content: string;
  tool_calls?: AiToolCall[];
  tool_call_id?: string;
  name?: string;
}

/// LLM provider 配置 (随请求传递, 后端不持久化密钥)。
export interface AiProviderConfig {
  adapter: string;
  base_url: string;
  api_key: string;
  model: string;
  temperature?: number | null;
  max_tokens?: number | null;
}

/// provider 适配器元数据 (设置 UI 下拉)。
export interface AiAdapterInfo {
  id: string;
  label: string;
  default_base_url: string;
}

/// 对话过程事件 (后端 Channel 推送, tag = "type")。
export type AiChatEvent =
  | { type: 'delta'; text: string }
  | { type: 'reasoning_delta'; text: string }
  | { type: 'tool_call'; id: string; name: string; arguments: unknown }
  | { type: 'tool_result'; id: string; name: string; content: string; is_error: boolean }
  | { type: 'done'; rounds: number }
  | { type: 'cancelled' }
  | {
      type: 'error';
      message: string;
      /** 错误种类 (后端 error::AppError::kind(), 供本地化; 旧事件为空) */
      kind?: string;
      /** 结构化字段 (adapter / model / rounds 等, 供本地化插值) */
      data?: Record<string, string>;
    };

/** 命令级错误 — 后端 AppError 的 IPC 序列化形态 */
export interface AiIpcError {
  kind: string;
  message: string;
  source?: { message: string };
  data?: Record<string, string>;
}

/// 外部 MCP server 传输方式。
export type McpTransport =
  | { kind: 'stdio'; command: string; args: string[]; env: Record<string, string> }
  | { kind: 'http'; url: string };

/// 外部 MCP server 配置。
export interface McpServerConfig {
  id: string;
  name: string;
  transport: McpTransport;
  enabled: boolean;
}

/// 聚合后的 MCP 工具信息 (前缀名)。
export interface McpToolInfo {
  server_id: string;
  server_name: string;
  prefixed_name: string;
  name: string;
  description: string;
  input_schema: unknown;
}

/// 本地 MCP server 状态。
export interface McpServerStatus {
  running: boolean;
  port: number | null;
}

/// 聊天面板中的工具调用记录 (UI 状态)。
export interface AiToolRun {
  id: string;
  name: string;
  arguments: unknown;
  content: string;
  is_error: boolean;
  /** 是否已收到结果 */
  done: boolean;
}

/// 对话视图条目 — 与后端 `ai_session::ViewItemDto` 对齐 (user / assistant 含工具卡片)。
export interface AiViewItem {
  role: 'user' | 'assistant';
  text: string;
  tools?: AiToolRun[];
  /** 错误条目 (仅 UI 展示, 不入 LLM 历史) */
  error?: boolean;
  /** 错误种类 (后端 error::AppError::kind(), 供本地化展示) */
  error_kind?: string;
  /** 错误结构化字段 (adapter / model / rounds 等) */
  error_data?: Record<string, string>;
}

/// 对话会话 — 与后端 `ai_session::ChatSession` 对齐 (历史所有权在后端)。
export interface AiChatSession {
  id: string;
  title: string;
  created_at: number;
  updated_at: number;
  items: AiViewItem[];
}

/// 会话摘要 — 与后端 `ai_session::SessionMeta` 对齐。
export interface AiSessionMeta {
  id: string;
  title: string;
  created_at: number;
  updated_at: number;
  item_count: number;
}
