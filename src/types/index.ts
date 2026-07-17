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

/// channels: null = 自动检测, number = 手动指定
export type ProtocolConfig =
  | { kind: 'JustFloat'; channels: number | null }
  | { kind: 'FireWater'; channels: number | null }
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

/// 后端 WaveformWindow — 与 serial-buffer 中 WaveformWindow 结构对应
export interface WaveformWindow {
  /// 相对最新时间戳的偏移 (毫秒, 负数=过去)
  timestamps: number[];
  /// 每通道的数据数组
  channels: number[][];
  /// 当前通道数
  channel_count: number;
}

// ============ 示波器风格轴配置 ============

/// 1-2-5 序列时基 (秒/格) — 100µs ~ 5s, 共 15 档
export const TIME_BASES_SEC: number[] = [
  100e-6, 200e-6, 500e-6,
  1e-3, 2e-3, 5e-3,
  10e-3, 20e-3, 50e-3,
  100e-3, 200e-3, 500e-3,
  1, 2, 5,
];

/// 1-2-5 序列 V/div (伏/格) — 1mV ~ 10000, 共 21 档
/// 覆盖极小信号到极大信号, 用户可通过手动输入或游标扩展更广范围
export const V_PER_DIV: number[] = [
  0.001, 0.002, 0.005,
  0.01, 0.02, 0.05,
  0.1, 0.2, 0.5,
  1, 2, 5,
  10, 20, 50,
  100, 200, 500,
  1000, 2000, 5000,
  10000,
];

/// 格式化时基 (秒/格) 为示波器风格字符串
export function formatTimeBase(sec: number): string {
  if (sec < 1e-3) return (sec * 1e6).toFixed(0) + 'µs/div';
  if (sec < 1) return (sec * 1e3).toFixed(0) + 'ms/div';
  return sec + 's/div';
}

/// 格式化 V/div 为示波器风格字符串
/// unit 默认为 'V', 但 Y 轴不一定是电压, 可传入任意单位 (如 'A' / '°C' / '')
/// 支持 µ/m 前缀 (小值) 和 k 前缀 (大值)
export function formatVPerDiv(v: number, unit = 'V'): string {
  const u = unit || '';
  if (v < 0.001) return (v * 1e6).toFixed(0) + 'µ' + u + '/div';
  if (v < 1) return (v * 1e3).toFixed(0) + 'm' + u + '/div';
  if (v >= 1000) return (v / 1e3).toFixed(0) + 'k' + u + '/div';
  return v + u + '/div';
}

/// 耦合方式
export type Coupling = 'DC' | 'AC' | 'GND';

/// 每通道独立配置
export interface ChannelAxisConfig {
  vPerDiv: number;        // V/格 (取自 V_PER_DIV)
  position: number;       // 垂直偏移 (伏, 屏幕中心 = 0)
  show: boolean;          // 通道可见性
  coupling: Coupling;     // 耦合方式
}

/// 游标测量配置
export interface CursorConfig {
  enabled: boolean;
  type: 'vertical' | 'horizontal';  // X 或 Y 游标
  c1: number;             // 第一条游标位置 (X=秒, Y=伏)
  c2: number;             // 第二条游标位置
}

/// 自动测量值
export interface ScopeMeasurements {
  vpp: number;
  vmin: number;
  vmax: number;
  vavg: number;
  vrms: number;
  freq: number | null;    // Hz, null=无法计算
  period: number | null;  // 秒
}

/// 示波器风格波形图配置 — 替代旧 WaveformAxisConfig
export interface ScopeAxisConfig {
  timeBase: number;       // 时基 (秒/格), 取自 TIME_BASES_SEC
  hPosition: number;      // 水平延迟 (秒, 0=实时, 正数=查看历史)
  channels: ChannelAxisConfig[];  // 每通道独立配置 (sharedY=true 时只使用 channels[0])
  grid: boolean;          // 网格可见
  running: boolean;       // true=运行 (持续更新), false=Stop (冻结)
  cursors: CursorConfig; // 游标
  yUnit: string;          // Y 轴单位 (不一定是电压, 如 'A'/'°C'/'', 默认 'V' 向后兼容)
  sharedY: boolean;       // true=所有通道共用一个 Y 轴 (共享 channels[0] 的 vPerDiv/position), 坐标轴显示真实值
}

/// 生成默认 ScopeAxisConfig (4 通道默认)
export function createDefaultScopeConfig(channelCount = 4): ScopeAxisConfig {
  return {
    timeBase: 100e-3,   // 100ms/div (默认显示 1 秒)
    hPosition: 0,       // 实时
    channels: Array.from({ length: channelCount }, () => ({
      vPerDiv: 1,        // 1V/div
      position: 0,
      show: true,
      coupling: 'DC' as Coupling,
    })),
    grid: true,
    running: true,
    cursors: {
      enabled: false,
      type: 'vertical',
      c1: -0.5,
      c2: 0.5,
    },
    yUnit: '',
    sharedY: true,      // 默认共用 Y (所有通道共享 V/div/position, 坐标轴显示真实值)
  };
}

/// 获取某通道的有效配置 — sharedY=true 时所有通道共用 channels[0] 的 vPerDiv/position
/// show/coupling 始终保持 per-channel 独立 (通道可见性与耦合方式不共用)
/// 用于归一化、反归一化、坐标轴显示等所有需要 vPerDiv/position 的场景
export function getEffectiveChannel(
  cfg: ScopeAxisConfig,
  idx: number
): ChannelAxisConfig {
  const fallback: ChannelAxisConfig = {
    vPerDiv: 1,
    position: 0,
    show: true,
    coupling: 'DC' as Coupling,
  };
  const own = cfg.channels[idx] ?? fallback;
  if (cfg.sharedY) {
    const shared = cfg.channels[0] ?? fallback;
    return {
      vPerDiv: shared.vPerDiv,
      position: shared.position,
      show: own.show,
      coupling: own.coupling,
    };
  }
  return own;
}

/// 计算波形图显示总时长 = 时基 × 10 格
export function timeBaseToWindowMs(timeBase: number): number {
  return timeBase * 10 * 1000;
}

/// 计算波形图垂直总范围 = V/div × 8 格 (上下各 4 格)
export function vPerDivToRange(vPerDiv: number): { min: number; max: number } {
  return { min: -vPerDiv * 4, max: vPerDiv * 4 };
}

// ============ 节点编辑器 ============

/// 节点端口类型
export type NodePortKind = 'input' | 'output';

/// 节点端口
export interface NodePort {
  id: string;
  kind: NodePortKind;
  label: string;
  channel?: number;
}

/// 节点位置 (兼容旧代码)
export interface NodePosition {
  x: number;
  y: number;
}

/// 节点连接 (兼容旧代码)
export interface NodeConnection {
  id: string;
  sourceNodeId: string;
  sourcePortId: string;
  targetNodeId: string;
  targetPortId: string;
}

/// 节点图边 — 与后端 vofa_next_buffer::graph::Edge 对应
export interface NodeGraphEdge {
  id: string;
  source: string;
  source_handle: string;
  target: string;
  target_handle: string;
}

/// 控件标签页
export interface ControlTab {
  id: string;
  name: string;
  widgets: string[]; // widget IDs in this tab
}

// ============ 数据显示区 Tab ============

export type DataTabType = 'waveform' | 'raw' | 'pie' | 'image' | 'waveform-extra';

export interface DataTab {
  id: string;
  type: DataTabType;
  name: string;
  widgetId?: string;
  closable: boolean;
}
