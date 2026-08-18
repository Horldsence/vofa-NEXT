import { invoke, Channel } from '@tauri-apps/api/core';
import type {
  CanLoadSnapshot,
  ConnectionState,
  DecoderBlock,
  FrameDecoderManualResult,
  InputFormat,
  PortInfo,
  ProtocolConfig,
  TransportConfig,
  TransportStats,
  WidgetBinding,
  WaveformWindow,
} from '../../types';
import type { NodeDef, GraphEdge } from '../utils/nodeDef';
import { clearRawDataBuffer } from '../buffers/rawDataSubscription';
import { makeLatestSink, subscribeSharded } from '../buffers/shardedSubscription';

/// 关闭 Tauri Channel 的完整流程:
/// 1. 调用后端 unsubscribe 命令, 从订阅者列表移除 (停止 send)
/// 2. 注销 JS 端回调 (cleanupCallback, 防止 callback id 残留)
/// 3. 清空 onmessage handler
///
/// 必须先调用后端移除再注销 JS 回调, 否则后端在 send 时找不到回调 ID 会产生警告。
export async function closeTauriChannel<T>(
  channel: Channel<T>,
  unsubscribeCmd?: string,
  channelId?: number
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
  channel.onmessage = () => {};
}

/// 数据管道性能配置 (snake_case, 与后端 PipelineConfig 对应)
/// 后端不持久化 — 前端在设置加载完成后重放, 更新即推送
export interface PipelineConfig {
  coalesce_max_msgs: number;
  coalesce_max_bytes_kb: number;
  max_feed_workers: number;
  feed_parallel_unit: number;
  min_worker_bytes_kb: number;
  max_stream_shards: number;
  parse_channel_cap: number;
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
  openTransport: (nodeId: string, config: TransportConfig, protocol: ProtocolConfig) =>
    invoke<void>('open_transport', { nodeId, config, protocol }),

  closeTransport: (nodeId: string) => invoke<void>('close_transport', { nodeId }),

  sendRaw: (nodeId: string, data: number[]) => invoke<void>('send_raw', { nodeId, data }),

  sendString: (nodeId: string, text: string) => invoke<void>('send_string', { nodeId, text }),

  /// protocolNode: Auto 编码所用的 Protocol 节点 id (Manual 模式可传 null)
  sendWidgetValue: (nodeId: string, protocolNode: string | null, binding: WidgetBinding, value: number) =>
    invoke<void>('send_widget_value', { nodeId, protocolNode, binding, value }),

  getConnectionState: (nodeId: string) => invoke<ConnectionState>('get_connection_state', { nodeId }),

  getStats: (nodeId: string) => invoke<TransportStats>('get_stats', { nodeId }),

  startTestData: (nodeId: string) => invoke<void>('start_test_data', { nodeId }),

  stopTestData: (nodeId: string) => invoke<void>('stop_test_data', { nodeId }),

  getTestDataState: (nodeId: string) => invoke<boolean>('get_test_data_state', { nodeId }),

  /// 协议回环: 发送字节并立即捕获指定 Protocol 节点的解析结果
  sendAndCapture: (nodeId: string, protocolNode: string, data: number[]) =>
    invoke<import('../../types').LoopbackResult>('send_and_capture', { nodeId, protocolNode, data }),

  /// 字节注入 — 从 sourceNodeId 的字节出口沿全局 BytePlan 路由到所有下游
  /// (FrameDecoder.in 喂入解析 / Protocol.in 喂入引擎 / Transport.tx 真实发送)
  /// 返回路由命中的下游数量 (0 = 未连线)
  injectBytes: (sourceNodeId: string, data: number[]) =>
    invoke<number>('inject_bytes', { sourceNodeId, data }),

  // ===== 协议 (nodeId = 图中 Protocol 节点 id) =====
  setProtocol: (nodeId: string, config: ProtocolConfig) =>
    invoke<void>('set_protocol', { nodeId, config }),

  getProtocol: (nodeId: string) => invoke<ProtocolConfig>('get_protocol', { nodeId }),

  /// 获取自动检测到的通道数 (仅在自动模式下返回 number, 否则 null)
  getDetectedChannels: (nodeId: string) => invoke<number | null>('get_detected_channels', { nodeId }),

  // ===== 波形缓冲区 (source = Protocol 节点 id) =====
  /// 订阅波形数据 — 统一分片流 (快照语义, 前端按 "最新 seq 胜出" 处理乱序)
  /// 返回一个取消订阅函数
  subscribeWaveform: (
    source: string,
    onEvent: (window: WaveformWindow) => void,
    options?: { intervalMs?: number; maxPoints?: number }
  ) => {
    return subscribeSharded<WaveformWindow>(
      'subscribe_waveform',
      'unsubscribe_waveform',
      { source },
      makeLatestSink(onEvent),
      { intervalMs: options?.intervalMs, maxPoints: options?.maxPoints }
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

  getBufferInfo: (source: string) => invoke<[number, number]>('get_buffer_info', { source }),

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
  /// 编译失败 (循环等) 返回错误, 旧图保留
  updateTabGraph: (tabId: string, nodes: NodeDef[], edges: GraphEdge[]) =>
    invoke<void>('update_tab_graph', { tabId, nodes, edges }),

  /// 移除指定 tab 的节点图 (tab 删除时调用)
  removeTabGraph: (tabId: string) =>
    invoke<void>('remove_tab_graph', { tabId }),

  // ===== CAN 负载分析 =====
  /// 获取 CAN 负载统计快照
  /// nodeId: 用于自动解析波特率的 Transport 节点 id
  /// bitrateBps: 可选手动覆盖波特率; null/0 = 自动从 TransportConfig 读取
  getCanLoadStats: (nodeId: string, bitrateBps?: number | null) =>
    invoke<CanLoadSnapshot>('get_can_load_stats', { nodeId, bitrateBps: bitrateBps ?? null }),

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
    options?: { intervalMs?: number; bitrateBps?: number | null }
  ) => {
    const channel = new Channel<CanLoadSnapshot>();
    channel.onmessage = onEvent;
    const promise = invoke<void>('subscribe_can_load', {
      nodeId,
      onEvent: channel,
      intervalMs: options?.intervalMs,
      bitrateBps: options?.bitrateBps ?? null,
    });
    return {
      promise,
      cancel: () => {
        void closeTauriChannel(channel, 'unsubscribe_can_load', channel.id);
      },
    };
  },

  /// 导出 CAN 负载统计为 CSV (自动保存到下载目录, 返回完整文件路径)
  /// nodeId: 用于自动解析波特率的 Transport 节点 id
  /// bitrateBps: 可选手动覆盖波特率; null/0 = 自动从 TransportConfig 读取
  exportCanLoadCsv: (nodeId: string, bitrateBps?: number | null) =>
    invoke<string>('export_can_load_csv', { nodeId, bitrateBps: bitrateBps ?? null }),

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

  // ===== 调试 =====
  /// 打开当前 Webview 的开发者工具（检查元素）
  inspectElement: () => invoke<void>('inspect_element'),

  // ===== 数据管道性能配置 =====
  setPipelineConfig: (config: PipelineConfig) =>
    invoke<void>('set_pipeline_config', { config }),

  getPipelineConfig: () => invoke<PipelineConfig>('get_pipeline_config'),
};
