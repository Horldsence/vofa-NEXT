import { invoke, Channel } from '@tauri-apps/api/core';
import type {
  ConnectionState,
  PortInfo,
  ProtocolConfig,
  TransportConfig,
  TransportStats,
  WidgetBinding,
  WaveformWindow,
} from '../types';
import type { NodeDef, GraphEdge } from './nodeDef';
import { clearRawDataBuffer } from './rawDataSubscription';

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

  startTestData: () => invoke<void>('start_test_data'),

  stopTestData: () => invoke<void>('stop_test_data'),

  getTestDataState: () => invoke<boolean>('get_test_data_state'),

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
    // 取消订阅: 先通知后端 task 退出, 再注销 JS 回调
    return {
      promise,
      cancel: () => {
        void closeTauriChannel(channel, 'unsubscribe_waveform', channel.id);
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

  setWaveformBufferCapacity: (maxPoints: number) =>
    invoke<void>('set_waveform_buffer_capacity', { maxPoints }),

  setRawDataBufferCapacity: (capacity: number) =>
    invoke<void>('set_rawdata_buffer_capacity', { capacity }),

  setCanBufferCapacity: (capacity: number) =>
    invoke<void>('set_can_buffer_capacity', { capacity }),

  setLogicBufferCapacity: (capacity: number) =>
    invoke<void>('set_logic_buffer_capacity', { capacity }),

  /// 清空后端原始数据收集器
  clearRawDataBuffer: () => clearRawDataBuffer(),

  // ===== 节点图 (后端化重构) =====
  /// 更新指定 tab 的节点图 (整体替换 nodes + edges)
  /// 编译失败 (循环等) 返回错误, 旧图保留
  updateTabGraph: (tabId: string, nodes: NodeDef[], edges: GraphEdge[]) =>
    invoke<void>('update_tab_graph', { tabId, nodes, edges }),

  /// 移除指定 tab 的节点图 (tab 删除时调用)
  removeTabGraph: (tabId: string) =>
    invoke<void>('remove_tab_graph', { tabId }),
};

