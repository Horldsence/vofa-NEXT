import { invoke, Channel } from '@tauri-apps/api/core';
import type {
  ConnectionState,
  PortInfo,
  ProtocolConfig,
  TransportConfig,
  TransportStats,
  WidgetBinding,
  WaveformWindow,
  NodeGraphEdge,
} from '../types';

export const api = {
  // ===== 传输 =====
  listPorts: () => invoke<PortInfo[]>('list_ports'),

  openTransport: (config: TransportConfig) =>
    invoke<void>('open_transport', { config }),

  closeTransport: () => invoke<void>('close_transport'),

  sendRaw: (data: number[]) => invoke<void>('send_raw', { data }),

  sendString: (text: string) => invoke<void>('send_string', { text }),

  sendWidgetValue: (binding: WidgetBinding, value: number) =>
    invoke<void>('send_widget_value', { binding, value }),

  getConnectionState: () => invoke<ConnectionState>('get_connection_state'),

  getStats: () => invoke<TransportStats>('get_stats'),

  // ===== 协议 =====
  setProtocol: (config: ProtocolConfig) =>
    invoke<void>('set_protocol', { config }),

  getProtocol: () => invoke<ProtocolConfig>('get_protocol'),

  /// 获取自动检测到的通道数 (仅在自动模式下返回 number, 否则 null)
  getDetectedChannels: () => invoke<number | null>('get_detected_channels'),

  // ===== 波形缓冲区 =====
  /// 订阅波形数据 — 通过 Tauri Channel 推送 WaveformWindow
  /// 返回一个取消订阅函数
  subscribeWaveform: (
    onEvent: (window: WaveformWindow) => void,
    options?: { intervalMs?: number; maxPoints?: number }
  ) => {
    const channel = new Channel<WaveformWindow>();
    channel.onmessage = onEvent;
    const promise = invoke<void>('subscribe_waveform', {
      onEvent: channel,
      intervalMs: options?.intervalMs,
      maxPoints: options?.maxPoints,
    });
    // 取消订阅: 关闭 channel 即可让后端任务退出
    return {
      promise,
      cancel: () => {
        // 重新赋值 onmessage 为空让 channel 自然关闭
        // Tauri Channel 没有显式 close API, 后端会在 send 失败时退出
        channel.onmessage = () => {};
      },
    };
  },

  /// 同步查询: 获取最近 N 个点
  getRecentWaveform: (count: number) =>
    invoke<WaveformWindow>('get_recent_waveform', { count }),

  /// 同步查询: 获取时间窗口内的数据 (相对最新时间的偏移, 毫秒)
  getWaveformWindow: (startMs: number, endMs: number) =>
    invoke<WaveformWindow>('get_waveform_window', {
      startMs,
      endMs,
    }),

  clearBuffer: () => invoke<void>('clear_buffer'),

  setBufferChannels: (count: number) =>
    invoke<void>('set_buffer_channels', { count }),

  getBufferInfo: () => invoke<[number, number]>('get_buffer_info'),

  // ===== 节点图 =====
  updateNodeGraph: (edges: NodeGraphEdge[]) =>
    invoke<void>('update_node_graph', { edges }),

  getNodeEdges: () => invoke<NodeGraphEdge[]>('get_node_edges'),
};
