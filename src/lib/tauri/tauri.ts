import type { Channel } from '@tauri-apps/api/core';
import { invoke } from '@tauri-apps/api/core';
import type {
  AiAdapterInfo,
  AiChatEvent,
  AiChatSession,
  AiProviderConfig,
  AiSessionMeta,
  CanLoadSnapshot,
  ConnectionState,
  DecoderBlock,
  FrameDecoderManualResult,
  InputFormat,
  McpServerConfig,
  McpServerStatus,
  McpToolInfo,
  PortInfo,
  ProtocolConfig,
  ProtocolSchema,
  TransportConfig,
  TransportStats,
  TriggerMatchResult,
  TriggerRule,
  WidgetBinding,
  WaveformWindow,
} from '../../types';
import type { NodeDef, GraphEdge } from '../utils/nodeDef';
import type { GraphDerivedPayload } from '../../store/slices/derived';
import { clearRawDataBuffer } from '../buffers/rawDataSubscription';
import {
  subscribeDisplay,
  subscribeDisplaySnapshot,
} from '../buffers/shardedSubscription';

/// 节点端口提示 — 与后端 `app_state::SourceNodeHint` 同形
/// (供拓扑 op 解析默认 handle / RawData `src:` 改写; 字段缺省时后端按类型兜底)
export interface SourceNodeHintPayload {
  default_input?: string;
  default_output?: string;
  raw_data?: boolean;
}

/// 画布坐标 — 与后端 `app_state::Position` / React Flow `node.position` 同形
export interface PositionPayload {
  x: number;
  y: number;
}

/// widget 配置记录 — 与后端 `app_state::WidgetRecord` 同形。
/// 前端 `WidgetConfig` 的透传存储 (kind + params), 后端不解释、原样回传
export interface WidgetRecordPayload {
  id: string;
  kind: string;
  params: Record<string, unknown>;
}

/// `graph:source` 事件 / get_source_graph 响应 — tab 权威源图
/// (含 widget 配置记录与画布位置: 画布据此重建该 tab 完整视图)
export interface GraphSourceEventPayload {
  tab_id: string;
  version: number;
  nodes: NodeDef[];
  edges: GraphEdge[];
  widgets?: WidgetRecordPayload[];
  positions?: Record<string, PositionPayload>;
}

/// 控件 tab 元数据 — 与后端 `app_state::TabMeta` 同形
export interface TabMetaPayload {
  id: string;
  name: string;
  widgets: string[];
}

/// 数据面板 tab 元数据 — 与后端 `app_state::DataTabMeta` 同形
export interface DataTabMetaPayload {
  id: string;
  name: string;
  type: string;
  closable: boolean;
  widget_id?: string | null;
}

/// `workspace_get` 响应 — 工作区水合快照 (null = 无持久化工作区, 默认启动)
export interface WorkspaceSnapshotPayload {
  version: number;
  tabs: TabMetaPayload[];
  data_tabs: DataTabMetaPayload[];
  graphs: {
    tab_id: string;
    nodes: NodeDef[];
    edges: GraphEdge[];
    widgets: WidgetRecordPayload[];
  }[];
  positions: Record<string, PositionPayload>;
}

/// connect_edge 响应
export interface ConnectedEdgePayload {
  edge_id: string;
}

/// disconnect_edge 响应
export interface DisconnectedEdgePayload {
  edge_id: string;
  source: string;
  target: string;
}

/// 关闭 Tauri Channel 的完整流程:
/// 1. 调用后端 unsubscribe 命令, 从订阅者列表移除 (停止 send)
/// 2. 注销 JS 端回调 (cleanupCallback, 防止 callback id 残留)
/// 3. 清空 onmessage handler
///
/// 必须先调用后端移除再注销 JS 回调, 否则后端在 send 时找不到回调 ID 会产生警告。
export async function closeTauriChannel<T>(
  channel: Channel<T>,
  unsubscribeCmd?: string,
  channelId?: number,
): Promise<void> {
  // 1. 通知后端移除 (如果在 HMR 期间后端已不可达, 忽略错误)
  if (unsubscribeCmd && channelId != null) {
    try {
      await invoke(unsubscribeCmd, { channelId });
    } catch {
      // 后端可能已不可达 (HMR/重载), 忽略
    }
  }
  // 2. 注销 JS 端回调
  const ch = channel as unknown as { cleanupCallback?: () => void };
  if (typeof ch.cleanupCallback === 'function') {
    ch.cleanupCallback();
  }
  // 3. 清空 handler
  channel.onmessage = () => { return undefined; };
}

/// 数据管道性能配置 (snake_case, 与后端 PipelineConfig 对应)
/// 后端不持久化 — 前端在设置加载完成后重放, 更新即推送
export interface PipelineConfig {
  mode: 'auto';
  max_workers: number;
  memory_budget_mb: number;
  preview_fps_limit: number;
  preview_bandwidth_mb_per_sec: number;
}

/// transport:state 事件 payload — 按节点分发
export interface TransportStateEvent {
  node_id: string;
  state: ConnectionState;
}

/// transport:rx 事件 payload — 按节点分发 (100ms 窗口增量统计)
export interface TransportRxEvent {
  node_id: string;
  stats: TransportStats;
}

export const api = {
  // ===== 传输 (nodeId = 图中 Transport 节点 id) =====
  listPorts: () => invoke<PortInfo[]>('list_ports'),

  /// 打开传输连接; protocol 仅被 TestData 用作生成数据的线缆格式参考
  /// schema: 可选帧 schema (custom 且带 encode 块时 TestData 按 schema 编码)
  openTransport: (
    nodeId: string,
    config: TransportConfig,
    protocol: ProtocolConfig,
    schema?: ProtocolSchema | null,
  ) =>
    invoke<void>('open_transport', {
      nodeId,
      config,
      protocol,
      schema: schema ?? null,
    }),

  closeTransport: (nodeId: string) =>
    invoke<void>('close_transport', { nodeId }),

  sendRaw: (nodeId: string, data: number[]) =>
    invoke<void>('send_raw', { nodeId, data }),

  sendString: (nodeId: string, text: string) =>
    invoke<void>('send_string', { nodeId, text }),

  /// TextOut 手动发送: 立即把图内当前文本发往该 TextOut 节点的目标 Transport
  sendTextOutNow: (nodeId: string) =>
    invoke<void>('send_text_out_now', { nodeId }),

  /// protocolNode: Auto 编码所用的 Protocol 节点 id (Manual 模式可传 null)
  sendWidgetValue: (
    nodeId: string,
    protocolNode: string | null,
    binding: WidgetBinding,
    value: number,
  ) =>
    invoke<void>('send_widget_value', { nodeId, protocolNode, binding, value }),

  getConnectionState: (nodeId: string) =>
    invoke<ConnectionState>('get_connection_state', { nodeId }),

  getStats: (nodeId: string) => invoke<TransportStats>('get_stats', { nodeId }),

  startTestData: (nodeId: string) =>
    invoke<void>('start_test_data', { nodeId }),

  stopTestData: (nodeId: string) => invoke<void>('stop_test_data', { nodeId }),

  getTestDataState: (nodeId: string) =>
    invoke<boolean>('get_test_data_state', { nodeId }),

  /// 运行时热更新传输节点的链路协议 (图/协议变化后推送, 无需重连)
  /// schema: 可选帧 schema (与 openTransport 语义一致)
  updateTransportProtocol: (
    nodeId: string,
    protocol: ProtocolConfig,
    schema?: ProtocolSchema | null,
    testDataConfig?:
      Extract<TransportConfig, { kind: 'TestData' }>['params'] | null,
  ) =>
    invoke<void>('update_transport_protocol', {
      nodeId,
      protocol,
      schema: schema ?? null,
      testDataConfig: testDataConfig ?? null,
    }),

  /// 协议回环: 发送字节并立即捕获指定 Protocol 节点的解析结果
  sendAndCapture: (nodeId: string, protocolNode: string, data: number[]) =>
    invoke<import('../../types').LoopbackResult>('send_and_capture', {
      nodeId,
      protocolNode,
      data,
    }),

  /// 字节注入 — 从 sourceNodeId 的字节出口沿全局 BytePlan 路由到所有下游
  /// (FrameDecoder.in 喂入解析 / Protocol.in 喂入引擎 / Transport.tx 真实发送)
  /// 返回路由命中的下游数量 (0 = 未连线)
  injectBytes: (sourceNodeId: string, data: number[]) =>
    invoke<number>('inject_bytes', { sourceNodeId, data }),

  /// 命令发送帧字节打包 — 后端 `compute_command_frame_bytes` IPC 单一权威
  /// (后端 cmd_buffer/src/command_frame.rs::compute_frame_bytes, 与前端纯预览分离)
  computeFrameBytes: (
    frame: import('../../types').CommandFrame,
    inputs: Record<string, number>,
  ) =>
    invoke<{
      bytes: number[] | null;
      error: string | null;
      per_block: number[][];
    }>('compute_command_frame_bytes', { frame, inputs }),

  // ===== 协议 (nodeId = 图中 Protocol 节点 id) =====
  setProtocol: (nodeId: string, config: ProtocolConfig) =>
    invoke<void>('set_protocol', { nodeId, config }),

  getProtocol: (nodeId: string) =>
    invoke<ProtocolConfig>('get_protocol', { nodeId }),

  /// 获取自动检测到的通道数 (仅在自动模式下返回 number, 否则 null)
  getDetectedChannels: (nodeId: string) =>
    invoke<number | null>('get_detected_channels', { nodeId }),

  // ===== 波形缓冲区 (source = Protocol 节点 id) =====
  /// 订阅波形数据 — 后端有序快照流
  /// 返回一个取消订阅函数
  subscribeWaveform: (
    source: string,
    onEvent: (window: WaveformWindow) => void,
    options?: { intervalMs?: number; maxPoints?: number },
  ) => {
    return subscribeDisplay<WaveformWindow>(
      { kind: 'waveform', source },
      'waveform',
      onEvent,
      { intervalMs: options?.intervalMs, maxItems: options?.maxPoints },
    );
  },

  /// 同步查询: 获取最近 N 个点
  getRecentWaveform: (source: string, count: number) =>
    invoke<WaveformWindow>('get_recent_waveform', { source, count }),

  /// 同步查询: 获取时间窗口内的数据 (相对最新时间的偏移, 毫秒)
  getWaveformWindow: (source: string, startMs: number, endMs: number) =>
    invoke<WaveformWindow>('get_waveform_window', {
      source,
      startMs,
      endMs,
    }),

  clearBuffer: (source: string) => invoke<void>('clear_buffer', { source }),

  setBufferChannels: (source: string, count: number) =>
    invoke<void>('set_buffer_channels', { source, count }),

  getBufferInfo: (source: string) =>
    invoke<[number, number]>('get_buffer_info', { source }),

  setWaveformBufferCapacity: (source: string, maxPoints: number) =>
    invoke<void>('set_waveform_buffer_capacity', { source, maxPoints }),

  setRawDataBufferCapacity: (source: string, capacity: number) =>
    invoke<void>('set_rawdata_buffer_capacity', { source, capacity }),

  setCanBufferCapacity: (capacity: number) =>
    invoke<void>('set_can_buffer_capacity', { capacity }),

  setLogicBufferCapacity: (capacity: number) =>
    invoke<void>('set_logic_buffer_capacity', { capacity }),

  /// 清空后端原始数据收集器 (source = Transport 节点 id; 缺省清空全部源)
  clearRawDataBuffer: (source?: string) => clearRawDataBuffer(source),

  // ===== 节点图 (后端化重构) =====
  /// 更新指定 tab 的节点图 (整体替换 nodes + edges; nodes 可含全局 Transport/Protocol 节点定义)
  /// nodeHints: 每节点端口提示 (后端拓扑 op 解析默认 handle / RawData 改写用)
  /// widgetRecords: 该 tab 全部 widget 配置记录 (配置模型的后端权威存储, 整体替换)
  /// positions: 节点画布位置 (合并进后端工作区位置表)
  /// baseVersion: 乐观并发基线 (期间被其他写入方推进则返回 GraphVersionConflict)
  /// 编译失败 (循环/域不匹配等) 返回真实原因, 旧图保留
  /// 返回 `GraphDerivedPayload` — 本次图变化涉及的全部节点派生端口表 / 通道数 + 新版本号
  updateTabGraph: (
    tabId: string,
    nodes: NodeDef[],
    edges: GraphEdge[],
    nodeHints?: Record<string, SourceNodeHintPayload>,
    baseVersion?: number | null,
    widgetRecords?: WidgetRecordPayload[],
    positions?: Record<string, PositionPayload>,
  ) =>
    invoke<GraphDerivedPayload>('update_tab_graph', {
      tabId,
      nodes,
      edges,
      nodeHints: nodeHints ?? {},
      widgets: widgetRecords ?? null,
      positions: positions ?? null,
      baseVersion: baseVersion ?? null,
    }),

  /// 上报节点画布位置 (拖拽结束时批量提交) — 轻量路径, 不触发编译
  setNodePositions: (positions: Record<string, PositionPayload>) =>
    invoke<void>('set_node_positions', { positions }),

  /// 读取工作区水合快照; null = 无持久化工作区 (全新安装, 前端走默认启动)
  workspaceGet: () => invoke<WorkspaceSnapshotPayload | null>('workspace_get'),

  /// 提交 tab 元数据 (控件 tab + 数据面板 tab) — 增删/改名/重排后整表覆盖
  workspaceSetTabs: (tabs: TabMetaPayload[], dataTabs: DataTabMetaPayload[]) =>
    invoke<void>('workspace_set_tabs', { tabs, dataTabs }),

  /// 读取指定 tab 的权威源图 (版本冲突后拉取合并; tab 无源图时返回 null)
  getSourceGraph: (tabId: string) =>
    invoke<GraphSourceEventPayload | null>('get_source_graph', { tabId }),

  /// 连线 — 后端权威入口 (编译校验, 失败返回真实原因且不建边)
  connectEdge: (params: {
    source: string;
    target: string;
    tabId?: string | null;
    sourceHandle?: string | null;
    targetHandle?: string | null;
  }) =>
    invoke<ConnectedEdgePayload>('connect_edge', {
      source: params.source,
      target: params.target,
      tabId: params.tabId ?? null,
      sourceHandle: params.sourceHandle ?? null,
      targetHandle: params.targetHandle ?? null,
    }),

  /// 删线 — 按 edgeId 或 source/target 组合 (可只给一端)
  disconnectEdge: (params: {
    edgeId?: string | null;
    source?: string | null;
    target?: string | null;
  }) =>
    invoke<DisconnectedEdgePayload>('disconnect_edge', {
      edgeId: params.edgeId ?? null,
      source: params.source ?? null,
      target: params.target ?? null,
    }),

  /// 移除指定 tab 的节点图 (tab 删除时调用)
  /// 返回 `GraphDerivedPayload` — 删除后剩余全局节点的派生数据 (供前端 derivedPorts 对账)
  removeTabGraph: (tabId: string) =>
    invoke<GraphDerivedPayload>('remove_tab_graph', { tabId }),

  // ===== CAN 负载分析 =====
  /// 获取 CAN 负载统计快照
  /// nodeId: 用于自动解析波特率的 Transport 节点 id
  /// bitrateBps: 可选手动覆盖波特率; null/0 = 自动从 TransportConfig 读取
  getCanLoadStats: (nodeId: string, bitrateBps?: number | null) =>
    invoke<CanLoadSnapshot>('get_can_load_stats', {
      nodeId,
      bitrateBps: bitrateBps ?? null,
    }),

  /// 设置 CAN 负载统计滑动窗口大小 (微秒)
  setCanLoadWindow: (windowUs: number) =>
    invoke<void>('set_can_load_window', { windowUs }),

  /// 清空 CAN 负载统计
  clearCanLoadStats: () => invoke<void>('clear_can_load_stats'),

  /// 获取当前 CAN 波特率 (从 TransportConfig 提取)
  /// 返回 [bps, source] — source = "slcan" / "candle" / "default"
  getCurrentCanBitrate: (nodeId: string) =>
    invoke<[number, string]>('get_current_can_bitrate', { nodeId }),

  /// 订阅 CAN 负载统计推送 — 周期性推送 CanLoadSnapshot
  /// nodeId: 用于自动解析波特率的 Transport 节点 id
  /// intervalMs: 推送间隔 (默认 500ms)
  /// bitrateBps: 可选手动覆盖波特率; null/0 = 自动从 TransportConfig 读取
  subscribeCanLoad: (
    nodeId: string,
    onEvent: (snap: CanLoadSnapshot) => void,
    options?: { intervalMs?: number; bitrateBps?: number | null },
  ) => {
    const subscription = subscribeDisplaySnapshot<CanLoadSnapshot>(
      {
        kind: 'can_load',
        node_id: nodeId,
        bitrate_bps: options?.bitrateBps ?? null,
      },
      'can_load',
      onEvent,
      options?.intervalMs ?? 500,
    );
    return {
      promise: Promise.resolve(),
      cancel: subscription.cancel,
    };
  },

  /// 导出 CAN 负载统计为 CSV (自动保存到下载目录, 返回完整文件路径)
  /// nodeId: 用于自动解析波特率的 Transport 节点 id
  /// bitrateBps: 可选手动覆盖波特率; null/0 = 自动从 TransportConfig 读取
  exportCanLoadCsv: (nodeId: string, bitrateBps?: number | null) =>
    invoke<string>('export_can_load_csv', {
      nodeId,
      bitrateBps: bitrateBps ?? null,
    }),

  // ===== 帧解码器手动测试 =====
  /// 解析用户输入字符串为帧 (使用 blocks 配置创建临时 FrameParser, 调用 parse_once)
  /// 返回 outputs (端口→值) + valid + consumedBytes + 可选 error
  parseFrameDecoderInput: (
    blocks: DecoderBlock[],
    input: string,
    format: InputFormat,
    enableValid: boolean,
    enableFrameCount: boolean,
    enableLastTimestamp: boolean,
    enableFps: boolean,
  ) =>
    invoke<FrameDecoderManualResult>('parse_frame_decoder_input', {
      blocks,
      input,
      format,
      enableValid,
      enableFrameCount,
      enableLastTimestamp,
      enableFps,
    }),

  // ===== 触发器匹配 (Trigger 面板) =====
  /// 在后端按 rules 表对 command 执行匹配, 返回首个命中规则的 outputValue/text;
  /// 全部未命中则返回 `{ value: defaultMiss, text: defaultMissText, matched: false, outputType: 'miss' }`
  /// - `numeric`: 用于 range 类型的规则; 传 null 跳过 range 规则
  /// 顶层参数用 camelCase (Tauri 命令参数约定), 规则字段转换为 snake_case (对齐 Rust serde 命名)
  matchTriggerCommand: (
    rules: TriggerRule[],
    defaultMiss: number,
    defaultMissText: string,
    command: string,
    numeric: number | null,
  ) =>
    invoke<TriggerMatchResult>('match_trigger_command', {
      rules: rules.map((r) => ({
        id: r.id,
        pattern: r.pattern,
        match_type: r.matchType,
        flags: r.flags ?? null,
        output_type: r.outputType,
        output_value: r.outputValue,
        output_text: r.outputText,
        enabled: r.enabled,
      })),
      defaultMiss,
      defaultMissText,
      command,
      numeric,
    }),

  // ===== 调试 =====
  /// 打开当前 Webview 的开发者工具（检查元素）
  inspectElement: () => invoke<void>('inspect_element'),

  // ===== 数据管道性能配置 =====
  setPipelineConfig: (config: PipelineConfig) =>
    invoke<void>('set_pipeline_config', { config }),

  getPipelineConfig: () => invoke<PipelineConfig>('get_pipeline_config'),

  // ===== AI 对话 + MCP =====
  /// 支持的 LLM provider 适配器清单 (设置 UI 下拉)
  aiListProviders: () => invoke<AiAdapterInfo[]>('ai_list_providers'),

  /// 发起一次对话 (可含多轮工具调用); 增量事件经 onEvent 推送, 返回 task_id。
  /// 会话所有权在后端: text 非空时追加用户条目, regenerate 时截断待重试回合。
  /// useBuiltinTools 启用内置原生工具 (软件自有能力 + 知识库, uiLang 注入系统提示词)
  aiChatSend: (
    sessionId: string,
    text: string | null,
    regenerate: boolean,
    config: AiProviderConfig,
    system: string | null,
    maxToolRounds: number,
    useMcpTools: boolean,
    useBuiltinTools: boolean,
    uiLang: string,
    onEvent: Channel<AiChatEvent>,
  ) =>
    invoke<string>('ai_chat_send', {
      sessionId,
      text,
      regenerate,
      config,
      system: system ?? null,
      maxToolRounds,
      useMcpTools,
      useBuiltinTools,
      uiLang,
      onEvent,
    }),

  /// 取消进行中的对话任务, 返回任务是否存在
  aiChatCancel: (taskId: string) =>
    invoke<boolean>('ai_chat_cancel', { taskId }),

  /// 前端托管工具回执 (toolHost 执行完 ai_tool_invoke 后调用);
  /// 返回是否存在对应 pending 调用 (超时清理后为 false)
  aiToolResolve: (callId: string, ok: boolean, result: unknown) =>
    invoke<boolean>('ai_tool_resolve', { callId, ok, result }),

  /// 全部对话会话摘要
  chatListSessions: () => invoke<AiSessionMeta[]>('chat_list_sessions'),

  /// 新建会话
  chatCreateSession: (title: string) =>
    invoke<AiChatSession>('chat_create_session', { title }),

  /// 读取单个会话 (含全部条目); 不存在返回 null
  chatGetSession: (sessionId: string) =>
    invoke<AiChatSession | null>('chat_get_session', { sessionId }),

  /// 重命名会话
  chatRenameSession: (sessionId: string, title: string) =>
    invoke<void>('chat_rename_session', { sessionId, title }),

  /// 删除会话
  chatDeleteSession: (sessionId: string) =>
    invoke<void>('chat_delete_session', { sessionId }),

  /// 清空会话条目 (保留会话本身)
  chatClearSession: (sessionId: string) =>
    invoke<void>('chat_clear_session', { sessionId }),

  /// 读取适配器的 API key (系统钥匙串); 未设置返回 null
  aiKeychainGet: (adapter: string) =>
    invoke<string | null>('ai_keychain_get', { adapter }),

  /// 写入适配器的 API key (系统钥匙串, 覆盖旧值)
  aiKeychainSet: (adapter: string, key: string) =>
    invoke<void>('ai_keychain_set', { adapter, key }),

  /// 删除适配器的 API key (不存在时静默)
  aiKeychainDelete: (adapter: string) =>
    invoke<void>('ai_keychain_delete', { adapter }),

  /// 全部外部 MCP server 配置
  mcpListServers: () => invoke<McpServerConfig[]>('mcp_list_servers'),

  /// 新增外部 MCP server 配置
  mcpAddServer: (config: McpServerConfig) =>
    invoke<void>('mcp_add_server', { config }),

  /// 删除外部 MCP server 配置 (同时断连)
  mcpRemoveServer: (id: string) => invoke<void>('mcp_remove_server', { id }),

  /// 启用 / 禁用外部 MCP server
  mcpSetServerEnabled: (id: string, enabled: boolean) =>
    invoke<void>('mcp_set_server_enabled', { id, enabled }),

  /// 刷新聚合工具列表 (自动连接已启用未连接的 server) 并更新对话工具缓存
  mcpListTools: () => invoke<McpToolInfo[]>('mcp_list_tools'),

  /// 各 server 的连接状态 [(server_id, connected)]
  mcpConnectionStates: () =>
    invoke<[string, boolean][]>('mcp_connection_states'),

  /// 手动调用一个聚合工具 (前缀名)
  mcpCallTool: (name: string, args: unknown) =>
    invoke<string>('mcp_call_tool', { name, arguments: args }),

  /// 本地 MCP server 状态
  mcpServerStatus: () => invoke<McpServerStatus>('mcp_server_status'),

  /// 启动本地 MCP server (返回实际端口; 已运行则直接返回)
  mcpServerStart: (port: number) =>
    invoke<number>('mcp_server_start', { port }),

  /// 停止本地 MCP server (未运行时静默)
  mcpServerStop: () => invoke<void>('mcp_server_stop'),
};
