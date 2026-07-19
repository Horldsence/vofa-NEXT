// ============ 传输层类型 ============

import type { SlcanConfig, CandleConfig } from './can';
import type { LogicDecoderConfig } from './logic';

export type ConnectionState = 'Disconnected' | 'Connecting' | 'Connected' | 'Error';

export interface PortInfo {
  name: string;
  port_type: string;
  vid: number | null;
  pid: number | null;
  serial_number: string | null;
  manufacturer: string | null;
  product: string | null;
}

export interface TransportStats {
  rx_bytes: number;
  tx_bytes: number;
  rx_frames: number;
  tx_frames: number;
}

export interface SerialConfig {
  port_name: string;
  baud_rate: number;
  data_bits: number;
  parity: 'none' | 'odd' | 'even';
  stop_bits: 'one' | 'two';
  flow_control: 'none' | 'software' | 'hardware';
}

export interface UdpConfig {
  local_addr: string;
  remote_addr: string;
  local_port: number;
  remote_port: number;
}

export interface TcpClientConfig {
  host: string;
  port: number;
}

export interface TcpServerConfig {
  listen_addr: string;
  listen_port: number;
}

export interface TestDataConfig {
  channels: number;
  sample_rate: number;
  signal: 'Sine' | 'Square' | 'Triangle' | 'Sawtooth' | 'Random' | 'Dc' | 'Chirp' | 'Steps' | 'Noise' | 'MultiTone';
}

export type TransportConfig =
  | { kind: 'Serial'; params: SerialConfig }
  | { kind: 'Udp'; params: UdpConfig }
  | { kind: 'TcpClient'; params: TcpClientConfig }
  | { kind: 'TcpServer'; params: TcpServerConfig }
  | { kind: 'TestData'; params: TestDataConfig }
  | { kind: 'Slcan'; params: SlcanConfig }
  | { kind: 'CandleLight'; params: CandleConfig };

// ============ 协议层类型 ============

/// channels: null = 自动检测, number = 手动指定
export type ProtocolConfig =
  | { kind: 'JustFloat'; channels: number | null }
  | { kind: 'FireWater'; channels: number | null }
  | { kind: 'RawData' }
  | { kind: 'Slcan' }
  | { kind: 'CandleLight' }
  | { kind: 'LogicDecode'; decoder: LogicDecoderConfig };
