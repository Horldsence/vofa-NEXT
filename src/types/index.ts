// ============ 传输层类型 ============

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
  signal: 'sine' | 'square' | 'triangle' | 'sawtooth' | 'random' | 'dc' | 'chirp' | 'steps' | 'noise' | 'multitone';
}

export type TransportConfig =
  | { kind: 'Serial'; params: SerialConfig }
  | { kind: 'Udp'; params: UdpConfig }
  | { kind: 'TcpClient'; params: TcpClientConfig }
  | { kind: 'TcpServer'; params: TcpServerConfig }
  | { kind: 'TestData'; params: TestDataConfig };

// ============ 协议层类型 ============

export type ProtocolConfig =
  | { kind: 'JustFloat'; channels: number }
  | { kind: 'FireWater'; channels: number }
  | { kind: 'RawData' };

// ============ 数据帧 ============

export interface DataFrame {
  timestamp: number;
  channels: number[];
}

export interface RawData {
  timestamp: number;
  data: number[];
}

// ============ 控件绑定 ============

export type WidgetBinding =
  | { mode: 'None' }
  | { mode: 'Auto'; params: { channel: number } }
  | { mode: 'Manual'; params: { template: string } };

// ============ 控件配置 ============

export interface KnobConfig {
  id: string;
  label: string;
  min: number;
  max: number;
  step: number;
  default: number;
  binding: WidgetBinding;
}

export interface ButtonConfig {
  id: string;
  label: string;
  press_value: number;
  release_value: number;
  binding: WidgetBinding;
}

export interface RadioConfig {
  id: string;
  label: string;
  options: [string, number][];
  default: number;
  binding: WidgetBinding;
}

export interface CheckboxConfig {
  id: string;
  label: string;
  checked_value: number;
  unchecked_value: number;
  default: boolean;
  binding: WidgetBinding;
}

export interface SliderConfig {
  id: string;
  label: string;
  min: number;
  max: number;
  step: number;
  default: number;
  binding: WidgetBinding;
}

export interface LabelConfig {
  id: string;
  text: string;
  channel: number | null;
}

export interface WaveformConfig {
  id: string;
  channels: number;
  max_points: number;
  visible_channels: boolean[];
}

export interface PieChartConfig {
  id: string;
  label: string;
  segments: string[];
  channels: number[];
}

export interface ImageConfig {
  id: string;
  label: string;
  width: number;
  height: number;
  format: 'rgb888' | 'rgb565' | 'gray8';
}

export type WidgetConfig =
  | { kind: 'Knob'; params: KnobConfig }
  | { kind: 'Button'; params: ButtonConfig }
  | { kind: 'Radio'; params: RadioConfig }
  | { kind: 'Checkbox'; params: CheckboxConfig }
  | { kind: 'Slider'; params: SliderConfig }
  | { kind: 'Label'; params: LabelConfig }
  | { kind: 'Waveform'; params: WaveformConfig }
  | { kind: 'PieChart'; params: PieChartConfig }
  | { kind: 'Image'; params: ImageConfig };

// ============ 波形数据 ============

export interface WaveformData {
  timestamps: number[];
  channels: number[][];
}
