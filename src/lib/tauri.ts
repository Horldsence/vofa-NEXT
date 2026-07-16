import { invoke } from '@tauri-apps/api/core';
import type {
  ConnectionState,
  PortInfo,
  ProtocolConfig,
  TransportConfig,
  TransportStats,
  WidgetBinding,
} from '../types';

export const api = {
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

  setProtocol: (config: ProtocolConfig) =>
    invoke<void>('set_protocol', { config }),

  getProtocol: () => invoke<ProtocolConfig>('get_protocol'),
};
