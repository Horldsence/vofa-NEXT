import type { ReactElement } from 'react';

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

// ============ 数据帧 ============

export interface DataFrame {
  timestamp: number;
  channels: number[];
}

export interface RawData {
  timestamp: number;
  data: number[];
}

/// 原始数据分片 — 与 Rust RawDataChunk 对应
export interface RawDataChunk {
  /// 微秒时间戳
  timestamp_us: number;
  /// 数据字节
  bytes: number[];
}

/// 原始数据批次 — 与 Rust RawDataBatch 对应
export interface RawDataBatch {
  chunks: RawDataChunk[];
  total_bytes: number;
  dropped_bytes: number;
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
  /// 动态 series 开关 — false(默认) 固定 widget.params.channels 通道槽 + 派生槽;
  /// true 时按实际连接数决定 series 数 (未连接通道不显示)
  dynamicSeries?: boolean;
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

/// 仪表盘控件 — 半圆指针式显示单通道实时值
export interface GaugeConfig {
  id: string;
  label: string;
  min: number;
  max: number;
  unit: string;          // 单位后缀, 如 'V' / 'A' / ''
  channel: number | null; // 绑定的输入通道 (null = 不绑定)
}

/// LED 指示灯 — 阈值控制开关色
export interface LEDConfig {
  id: string;
  label: string;
  threshold: number;     // 输入 >= threshold 视为 ON
  on_color: string;      // HEX, 如 '#89d185'
  off_color: string;     // HEX, 如 '#3c3c3c'
  channel: number | null;
}

/// 大数字显示 — 大字号展示单通道数值
export interface NumberDisplayConfig {
  id: string;
  label: string;
  unit: string;
  precision: number;     // 小数位数
  channel: number | null;
}

/// 自定义 JS 控件 — 用户代码在 iframe 沙箱中渲染
/// 代码格式见 src/components/displays/CustomWidget.tsx 顶部注释
export interface CustomConfig {
  id: string;
  label: string;
  code: string;           // JS 源码, 求值后应返回 widget 定义对象
  settings: Record<string, string | number | boolean>; // 用户在设置面板里填写的值
}

/// 算术控件 — 对多个通道输入做四则运算/数学函数, 输出单通道结果
/// 可串联使用: 上游 Math widget 的输出端口可接到下游 Math widget 的输入端口
export type MathOp =
  | 'add'      // 求和: a + b + ...
  | 'sub'      // 减法: a - b - ...
  | 'mul'      // 乘积: a × b × ...
  | 'div'      // 除法: a ÷ b ÷ ... (除数为 0 时返回 0)
  | 'avg'      // 平均值
  | 'min'      // 最小值
  | 'max'      // 最大值
  | 'abs'      // 绝对值 (仅第一输入)
  | 'neg'      // 取反 (仅第一输入)
  | 'square'   // 平方 (仅第一输入)
  | 'sqrt'     // 平方根 (仅第一输入)
  | 'sin'      // 正弦 (仅第一输入, 弧度)
  | 'cos'      // 余弦 (仅第一输入, 弧度)
  | 'tan'      // 正切 (仅第一输入, 弧度)
  | 'log';     // 自然对数 (仅第一输入, ≤0 返回 0)

export interface MathConfig {
  id: string;
  label: string;
  op: MathOp;
  inputCount: number;     // 输入端口数 (1 ~ 8), 单目运算固定为 1
  unit: string;          // 单位后缀, 如 'V' / '' (用于显示)
  precision: number;      // 输出小数位
}

// ============ 频域 DSP 类型 ============
//
// 与 Rust vofa_next_dsp 对应, 使用 serde 默认 (externally-tagged) 表示:
//   { "FIR": { "b": [...] } }
//   { "IIR": { "b": [...], "a": [...] } }
//   { "Lowpass": { "cutoff": 100, "sample_rate": 1000 } }
//   { "Hann": null }  (unit variant)
//
// 这些类型通过 IPC 与后端交换, 字段名与 Rust 端 snake_case 一致

/// 窗函数类型 (与 Rust WindowType 对应)
export type WindowType = 'Rect' | 'Hann' | 'Hamming' | 'Blackman';

/// 频谱输出模式 (与 Rust SpectrumOutput 对应)
export type SpectrumOutput = 'Magnitude' | 'Power' | 'PSD' | 'Decibel';

/// 滤波器预设类型 (前端友好, 与 Rust FilterPreset 对应)
export type FilterPresetKind = 'Lowpass' | 'Highpass' | 'Bandpass' | 'Bandstop';

/// 滤波器配置 (前端友好形式, 同步到后端时转为 IIR biquad coeffs)
export interface FilterConfig {
  id: string;
  label: string;
  /// 预设类型 (低通/高通/带通/带阻)
  preset: FilterPresetKind;
  /// 截止频率 (Hz) — 用于 Lowpass/Highpass
  cutoff: number;
  /// 通带/阻带下限 (Hz) — 用于 Bandpass/Bandstop
  low: number;
  /// 通带/阻带上限 (Hz) — 用于 Bandpass/Bandstop
  high: number;
  /// 采样率 (Hz)
  sampleRate: number;
  /// 输出小数位 (显示用)
  precision: number;
}

/// 频谱分析配置
export interface SpectrumConfig {
  id: string;
  label: string;
  /// FFT 窗口大小 (2 的幂, 256/512/1024/2048)
  windowSize: number;
  /// 窗函数类型
  windowType: WindowType;
  /// 输出模式
  output: SpectrumOutput;
  /// 采样率 (Hz)
  sampleRate: number;
}

// ============ 3D 模型显示 ============

/// 3D 显示模式
/// - trajectory: 三通道作为 (x, y, z) 坐标, 渲染拖尾散点轨迹
/// - attitude:   三通道作为欧拉角 (roll=x, pitch=y, yaw=z, 弧度), 渲染姿态立方体
export type Model3DMode = 'trajectory' | 'attitude';

/// 3D 模型控件配置
/// 输入端口固定为 x / y / z, 缺失通道补 0
export interface Model3DConfig {
  id: string;
  label: string;
  /// 显示模式
  mode: Model3DMode;
  /// 拖尾长度 (trajectory 模式, 默认 200)
  trailLength: number;
  /// 拖尾/立方体颜色 (HEX, 如 '#75beff')
  color: string;
  /// 坐标轴长度 (默认 1.0)
  axisLength: number;
}

// ============ CAN 类型 ============

/// CAN 帧方向
export type CanDirection = 'Rx' | 'Tx';

/// CAN 帧 — 与 Rust CanFrame 对应
export interface CanFrame {
  /// 微秒时间戳
  timestamp: number;
  /// CAN ID (11 位标准 / 29 位扩展)
  id: number;
  /// true = 扩展帧
  extended: boolean;
  /// true = 远程帧
  rtr: boolean;
  /// 数据长度 (0-8)
  dlc: number;
  /// 数据字节
  data: number[];
  /// 收/发方向
  direction: CanDirection;
}

/// CAN 帧批次 — 与 Rust CanFrameBatch 对应
export interface CanFrameBatch {
  frames: CanFrame[];
}

/// candleLight USB 设备信息 — 与 Rust CandleDeviceInfo 对应
export interface CandleDeviceInfo {
  bus: number;
  address: number;
  vid: number;
  pid: number;
  manufacturer: string | null;
  product: string | null;
  serial_number: string | null;
}

/// CAN 波特率预设
export type CanBitrate = 'bps100k' | 'bps125k' | 'bps250k' | 'bps500k' | 'bps1m';

/// 单个 ID 的 CAN 负载统计 — 与 Rust CanIdLoadStats 对应
export interface CanIdLoadStats {
  id: number;
  extended: boolean;
  frame_count: number;
  total_bits: number;
  total_bytes: number;
}

/// CAN 负载历史采样点 — 与 Rust CanLoadHistoryPoint 对应
export interface CanLoadHistoryPoint {
  /// 微秒时间戳
  timestamp: number;
  /// 负载率 (0.0 - 1.0+)
  load_ratio: number;
  /// 帧率 (帧/秒)
  fps: number;
}

/// CAN 负载统计快照 — 与 Rust CanLoadSnapshot 对应
export interface CanLoadSnapshot {
  /// 窗口大小 (微秒)
  window_us: number;
  /// 窗口内总帧数
  frame_count: number;
  /// 窗口内总位数
  total_bits: number;
  /// 窗口内总字节数
  total_bytes: number;
  /// 当前负载率 (0.0 - 1.0+)
  load_ratio: number;
  /// 时序采样历史
  history: CanLoadHistoryPoint[];
  /// 按 ID 的负载分布 (按 total_bits 降序)
  per_id: CanIdLoadStats[];
  /// 按 ID 的负载率历史 (用于时序图叠加显示)
  per_id_history: CanIdLoadHistory[];
}

/// 单个 ID 的负载率历史 — 与 Rust CanIdLoadHistory 对应
export interface CanIdLoadHistory {
  id: number;
  extended: boolean;
  history: CanLoadHistoryPoint[];
}

/// slcan 传输配置
export interface SlcanConfig {
  port_name: string;
  baud_rate: number;
  can_bitrate: CanBitrate;
}

/// candleLight 传输配置
export interface CandleConfig {
  bus: number;
  address: number;
  can_bitrate: CanBitrate;
  channel: number;
}

// ============ 逻辑分析仪类型 ============

/// 逻辑采样 — 与 Rust LogicSample 对应
export interface LogicSample {
  /// 微秒时间戳
  timestamp: number;
  /// 通道位图, bit i = 通道 i 的电平 (0/1)
  channels: number;
  /// 实际通道数
  channel_count: number;
}

/// 逻辑采样批次 — 与 Rust LogicSampleBatch 对应
export interface LogicSampleBatch {
  samples: LogicSample[];
}

/// I2C 事件 — 与 Rust I2cEvent 对应
export type I2cEvent =
  | { Start: null }
  | { Stop: null }
  | { Address: { addr: number; read: boolean; ack: boolean } }
  | { Data: { byte: number; ack: boolean } };

/// 解码事件 — 与 Rust DecodedEvent 对应
/// 注意: Rust serde 用 externally-tagged, 形如 { "Uart": { timestamp, byte, parity_ok } }
export type DecodedEvent =
  | { Uart: { timestamp: number; byte: number; parity_ok: boolean } }
  | { I2c: { timestamp: number; event: I2cEvent } }
  | { Spi: { timestamp: number; mosi: number; miso: number } };

/// 解码事件批次 — 与 Rust DecodedEventBatch 对应
export interface DecodedEventBatch {
  events: DecodedEvent[];
}

/// 逻辑解码器配置 — 与 Rust LogicDecoderConfig 对应
export type LogicDecoderConfig =
  | { kind: 'Uart'; params: { baud_rate: number; data_bits: number; parity: 'none' | 'odd' | 'even'; stop_bits: 'one' | 'two'; channel: number } }
  | { kind: 'I2c'; params: { sda_channel: number; scl_channel: number } }
  | { kind: 'Spi'; params: { sclk_channel: number; mosi_channel: number; miso_channel: number; cs_channel: number; mode: number } };

// ============ 协议输入解析 ============

/// 输入格式 — 与 Rust InputFormat 对应 (serde rename_all="lowercase")
export type InputFormat = 'hex' | 'ascii' | 'auto';

// ============ 帧解码控件 (FrameDecoder) ============
//
// 镜像 CommandSender 的块列表设计, 但方向相反:
// - CommandSender: 块列表 → 拼接字节流 → 发送 (输入端口来自 var_ref 块)
// - FrameDecoder:  字节流 → 按块定义解析 → 输出端口 (每个 field/bitfield 块 → 一个输出端口)
//
// 数据来源:
// - 实时模式: 订阅全局 raw_data_collector, 解析传输层接收的字节流
// - 手动测试: 用户在 UI 中粘贴 HEX/ASCII 字符串进行一次性解析
//
// 块类型:
// - header:    匹配帧头固定字节序列 (帧起始标志)
// - length:    读 N 字节为整数, 输出到 length 端口 + 决定后续变长字段长度
// - id:        读 N 字节为整数, 输出到 id_value 端口 + 设置 match_id 上下文
// - field:     按 fieldType 读 N 字节并解码为 f32, 输出到 portName 端口
// - bitfield:  从指定字节按 bit 偏移+位长读取, 输出到 portName 端口
// - checksum:  对前序累计字节校验, 输出 valid 端口 (1.0/0.0)
// - tail:      匹配帧尾固定字节序列 (可选, 帧结束标志)

/// 帧解码块类型
export type DecoderBlockType =
  | 'header'
  | 'length'
  | 'id'
  | 'field'
  | 'bitfield'
  | 'checksum'
  | 'tail';

/// 帧解码校验位置 (与 CommandSender ChecksumPosition 区分, 避免序列化混淆)
/// - append:  校验字节位于帧末尾 (在 tail 之前)
/// - inline:  校验字节位于当前位置 (在块列表中该 checksum 块的位置)
/// - prepend: 校验字节位于帧头之后 (在 header 之后)
export type DecoderChecksumPosition = 'append' | 'inline' | 'prepend';

/// 帧解码块的覆盖范围 (校验计算的字节范围)
/// - all_prior: 从帧开头到本校验块之前的所有字节
/// - range:     用户指定字节偏移范围 [cover_start, cover_end)
export type DecoderChecksumCover = 'all_prior' | 'range';

/// 帧解码块定义 (与 Rust DecoderBlockDef 对应, serde tag="type" content="params")
///
/// 每个块可选 `match_id` 字段 (id 块除外) — 仅当当前帧的 id_value 等于 match_id 时该块执行
/// 未设置 match_id 的块始终执行 (用于多帧类型分派)
export type DecoderBlock =
  | {
      id: string;
      type: 'header';
      label?: string;
      /// 帧头 HEX 字符串 "AA BB"
      hex: string;
      /// 可选 match_id (用于多帧类型分派, 通常 header 不设)
      matchId?: number | null;
    }
  | {
      id: string;
      type: 'length';
      label?: string;
      /// 整数字段类型 (uint8/16/32 LE/BE)
      fieldType: FieldType;
      /// 输出端口名 (默认 "length")
      portName?: string;
      /// 长度单位: bytes=字节数, fields=后续 field 块重复次数
      unit?: 'bytes' | 'fields';
      matchId?: number | null;
    }
  | {
      id: string;
      type: 'id';
      label?: string;
      /// 整数字段类型
      fieldType: FieldType;
      /// 输出端口名 (默认 "id_value")
      portName?: string;
    }
  | {
      id: string;
      type: 'field';
      label?: string;
      /// 字段类型 (int/uint/float LE/BE, bytes)
      fieldType: FieldType;
      /// 输出端口名 (节点上暴露的 Handle id)
      portName: string;
      /// 若设置, 引用某个 length 块的 id — 该字段读取 length_value 字节而非 fieldType 固定长度
      /// (仅 fieldType='bytes' 时生效, 输出第一字节为 f32; 其他类型忽略此字段)
      lengthRef?: string | null;
      /// 仅当 id_value === matchId 时执行 (多帧分派)
      matchId?: number | null;
    }
  | {
      id: string;
      type: 'bitfield';
      label?: string;
      /// 字节偏移 (相对于当前解析位置)
      byteOffset: number;
      /// 位偏移 (0-7)
      bitOffset: number;
      /// 位长度 (1-32)
      bitLength: number;
      /// 是否带符号 (true=最高位为符号位, 二补码)
      isSigned: boolean;
      /// 输出端口名
      portName: string;
      matchId?: number | null;
    }
  | {
      id: string;
      type: 'checksum';
      label?: string;
      /// 校验算法
      algorithm: ChecksumType;
      /// 自定义脚本 (algorithm='custom' 时使用)
      customScript?: string;
      /// 校验覆盖范围
      cover: DecoderChecksumCover;
      /// cover='range' 时的起始字节偏移 (相对帧头之后)
      coverStart?: number;
      /// cover='range' 时的结束字节偏移 ( exclusive)
      coverEnd?: number;
      /// 校验字节在帧中的位置
      position: DecoderChecksumPosition;
      /// 仅当 id_value === matchId 时执行
      matchId?: number | null;
    }
  | {
      id: string;
      type: 'tail';
      label?: string;
      /// 帧尾 HEX 字符串
      hex: string;
      matchId?: number | null;
    };

/// 帧解码控件配置
export interface FrameDecoderConfig {
  id: string;
  label: string;
  /// 块列表 (按顺序定义帧布局)
  blocks: DecoderBlock[];
  /// 附加输出端口开关
  enableValid: boolean;        // 输出 valid 端口 (1.0=最近帧校验通过, 0.0=失败/未收到)
  enableFrameCount: boolean;   // 输出 frame_count 端口 (累计有效帧数)
  enableLastTimestamp: boolean; // 输出 last_timestamp 端口 (微秒)
  enableFps: boolean;          // 输出 fps 端口 (滑动窗口帧率)
  /// 数据模式: 'live' = 实时数据流, 'manual' = 手动测试
  mode: 'live' | 'manual';
}

/// 帧解码器手动测试结果 (与 Rust FrameDecoderParseResult 对应)
///
/// 由 parse_frame_decoder_input 命令返回, 用于 manual 模式下的单次解析。
/// - outputs: 端口名 → 值 (来自 field/bitfield/length/id 块 + 可选附加端口)
/// - valid: 校验是否通过
/// - consumedBytes: 本帧消耗的字节数
/// - error: 解析错误信息 (帧不完整/校验失败等)
export interface FrameDecoderManualResult {
  outputs: Record<string, number>;
  valid: boolean;
  consumedBytes: number;
  error?: string;
}

// ============ 命令发送控件 ============

/// 输入格式
/// - hex:         HEX 字节流, 空格分隔 (如 "AA 01 02 BB")
/// - ascii:       ASCII 文本 + 转义字符 (\n \t \r \xHH)
/// - template:    模板字符串 (${VAR} 变量插值)
/// - structured:  结构化字段定义 (按字节序打包)
export type CommandFormat = 'hex' | 'ascii' | 'template' | 'structured';

/// 校验算法类型 (与 lib/checksum.ts 对应)
export type ChecksumType =
  | 'none'
  | 'sum8'
  | 'xor8'
  | 'crc8'
  | 'crc16Modbus'
  | 'crc16CCITT'
  | 'crc32'
  | 'lrc'
  | 'custom';

/// 校验位置
/// - append:  追加到字节流末尾
/// - prepend: 插入到字节流开头
/// - none:    不附加 (仅用于显示)
export type ChecksumPosition = 'append' | 'prepend' | 'none';

/// 结构化字段类型
export type FieldType =
  | 'uint8'
  | 'int8'
  | 'uint16LE'
  | 'uint16BE'
  | 'int16LE'
  | 'int16BE'
  | 'uint32LE'
  | 'uint32BE'
  | 'int32LE'
  | 'int32BE'
  | 'float32LE'
  | 'float32BE'
  | 'bytes';

/// 结构化字段定义
export interface CommandField {
  id: string;
  name: string;
  type: FieldType;
  value: string;  // 字符串形式, 解析时转换
}

/// 数据块类型 — Command 以块列表方式拼接最终字节流
/// - const_hex:   固定字节序列 (如帧头 AA BB)
/// - var_ref:     引用输入端口的值, 按 fieldType 编码 (端口名自定义)
/// - typed_const: 手动输入的字面值, 按 fieldType 编码
/// - checksum:    对前面所有块的累计字节计算校验
export type BlockType = 'const_hex' | 'var_ref' | 'typed_const' | 'checksum';

/// 单个数据块
export interface CommandBlock {
  id: string;
  type: BlockType;
  /// 块显示名 (可选, 用于列表中标识)
  label?: string;
  /// const_hex: hex 字符串 "AA 01 02"
  hex?: string;
  /// var_ref: 自定义输入端口名 (如 "speed"), 节点上按此名暴露 Handle
  portName?: string;
  /// var_ref / typed_const: 数据编码类型
  fieldType?: FieldType;
  /// typed_const: 字面值字符串
  value?: string;
  /// checksum: 校验算法
  checksum?: ChecksumType;
  /// checksum: 自定义校验脚本 (checksum === 'custom' 时使用)
  customScript?: string;
}

/// 命令发送控件配置
/// 以数据块列表方式拼接最终字节流, 支持可变数量输入端口 + 校验
export interface CommandConfig {
  id: string;
  label: string;
  /// 数据块列表 (按顺序拼接为最终字节流)
  blocks: CommandBlock[];
  /// 发送后追加 \n
  appendNewline: boolean;
}

/// 频谱计算结果 — 与 Rust SpectrumResult 对应
export interface SpectrumResult {
  /// 频率 (Hz), 长度 = windowSize / 2 + 1
  frequencies: number[];
  /// 频谱值 (Magnitude/Power/PSD/Decibel), 与 frequencies 对齐
  values: number[];
}

/// 控件类别 — 用于 WidgetPalette 分组与颜色区分
export type WidgetCategory =
  | 'input'      // 数据类 (Knob/Button/Radio/Checkbox/Slider/Command)
  | 'display'    // 显示控件 (Waveform/PieChart/Image/Gauge/LED/NumberDisplay/Label/Spectrum/Model3D)
  | 'math'       // 算术控件 (Math/Filter — 加减乘除/数学函数/滤波)
  | 'custom';    // 自定义控件 (Custom JS)

export type WidgetConfig =
  | { kind: 'Knob'; params: KnobConfig }
  | { kind: 'Button'; params: ButtonConfig }
  | { kind: 'Radio'; params: RadioConfig }
  | { kind: 'Checkbox'; params: CheckboxConfig }
  | { kind: 'Slider'; params: SliderConfig }
  | { kind: 'Label'; params: LabelConfig }
  | { kind: 'Waveform'; params: WaveformConfig }
  | { kind: 'PieChart'; params: PieChartConfig }
  | { kind: 'Image'; params: ImageConfig }
  | { kind: 'Gauge'; params: GaugeConfig }
  | { kind: 'LED'; params: LEDConfig }
  | { kind: 'NumberDisplay'; params: NumberDisplayConfig }
  | { kind: 'Custom'; params: CustomConfig }
  | { kind: 'Math'; params: MathConfig }
  | { kind: 'Filter'; params: FilterConfig }
  | { kind: 'Spectrum'; params: SpectrumConfig }
  | { kind: 'Model3D'; params: Model3DConfig }
  | { kind: 'Command'; params: CommandConfig }
  | { kind: 'FrameDecoder'; params: FrameDecoderConfig };

/// 获取控件所属类别 (用于 palette 分组与着色)
export function getWidgetCategory(kind: WidgetConfig['kind']): WidgetCategory {
  switch (kind) {
    case 'Knob':
    case 'Button':
    case 'Radio':
    case 'Checkbox':
    case 'Slider':
    case 'Command':
      return 'input';
    case 'Waveform':
    case 'PieChart':
    case 'Image':
    case 'Gauge':
    case 'LED':
    case 'NumberDisplay':
    case 'Label':
    case 'Spectrum':
    case 'Model3D':
      return 'display';
    case 'Math':
    case 'Filter':
      return 'math';
    case 'Custom':
      return 'custom';
    case 'FrameDecoder':
      return 'input';
  }
}

/// 单目运算集合 — 这些 op 只使用第一个输入
export const UNARY_MATH_OPS: MathOp[] = ['abs', 'neg', 'square', 'sqrt', 'sin', 'cos', 'tan', 'log'];

// ============ Biquad 滤波器系数计算 (与 Rust RBJ Audio EQ Cookbook 一致) ============
//
// 前端在 syncTabGraph 时将 FilterConfig (preset + cutoff + sampleRate) 转为 IIR biquad 系数,
// 通过 IPC 同步到后端。后端 DigitalFilter::new(FilterKind::IIR { b, a }) 直接使用这些系数。
//
// 公式参考: https://www.musicdsp.org/en/latest/Filters/197-rbj-audio-eq-cookbook.html

const PI_F32 = Math.PI;
const DEFAULT_Q = 0.70710678; // 1/sqrt(2), Butterworth 响应

function w0(cutoff: number, sampleRate: number): number {
  return 2.0 * PI_F32 * cutoff / sampleRate;
}

function alpha(w0: number, q: number): number {
  return Math.sin(w0) / (2.0 * q);
}

/// 低通 biquad 系数 (fc=截止频率, fs=采样率)
export function lowpassBiquad(cutoff: number, sampleRate: number): { b: [number, number, number]; a: [number, number, number] } {
  const w = w0(cutoff, sampleRate);
  const a = alpha(w, DEFAULT_Q);
  const cosW = Math.cos(w);
  const b0 = (1.0 - cosW) / 2.0;
  const b1 = 1.0 - cosW;
  const b2 = (1.0 - cosW) / 2.0;
  const a0 = 1.0 + a;
  const a1 = -2.0 * cosW;
  const a2 = 1.0 - a;
  return { b: [b0, b1, b2], a: [a0, a1, a2] };
}

/// 高通 biquad 系数
export function highpassBiquad(cutoff: number, sampleRate: number): { b: [number, number, number]; a: [number, number, number] } {
  const w = w0(cutoff, sampleRate);
  const a = alpha(w, DEFAULT_Q);
  const cosW = Math.cos(w);
  const b0 = (1.0 + cosW) / 2.0;
  const b1 = -(1.0 + cosW);
  const b2 = (1.0 + cosW) / 2.0;
  const a0 = 1.0 + a;
  const a1 = -2.0 * cosW;
  const a2 = 1.0 - a;
  return { b: [b0, b1, b2], a: [a0, a1, a2] };
}

/// 带通 biquad 系数 (常量 0 dB 峰值)
/// low, high: 通带 [low, high]
export function bandpassBiquad(low: number, high: number, sampleRate: number): { b: [number, number, number]; a: [number, number, number] } {
  const fc = Math.sqrt(low * high);
  const bw = high - low;
  const w = w0(fc, sampleRate);
  const q = bw > 0 ? fc / bw : DEFAULT_Q;
  const a = alpha(w, q);
  const cosW = Math.cos(w);
  const b0 = a;
  const b1 = 0.0;
  const b2 = -a;
  const a0 = 1.0 + a;
  const a1 = -2.0 * cosW;
  const a2 = 1.0 - a;
  return { b: [b0, b1, b2], a: [a0, a1, a2] };
}

/// 带阻 (陷波) biquad 系数
export function bandstopBiquad(low: number, high: number, sampleRate: number): { b: [number, number, number]; a: [number, number, number] } {
  const fc = Math.sqrt(low * high);
  const bw = high - low;
  const w = w0(fc, sampleRate);
  const q = bw > 0 ? fc / bw : DEFAULT_Q;
  const a = alpha(w, q);
  const cosW = Math.cos(w);
  const b0 = 1.0;
  const b1 = -2.0 * cosW;
  const b2 = 1.0;
  const a0 = 1.0 + a;
  const a1 = -2.0 * cosW;
  const a2 = 1.0 - a;
  return { b: [b0, b1, b2], a: [a0, a1, a2] };
}

/// 根据 FilterConfig 计算对应的 IIR biquad 系数
/// 用于 widgetToNodeKind: 将前端友好的 preset/cutoff 形式转为后端 FilterKind::IIR
export function biquadFromFilterConfig(cfg: FilterConfig): { b: [number, number, number]; a: [number, number, number] } {
  switch (cfg.preset) {
    case 'Lowpass':  return lowpassBiquad(cfg.cutoff, cfg.sampleRate);
    case 'Highpass': return highpassBiquad(cfg.cutoff, cfg.sampleRate);
    case 'Bandpass': return bandpassBiquad(cfg.low, cfg.high, cfg.sampleRate);
    case 'Bandstop': return bandstopBiquad(cfg.low, cfg.high, cfg.sampleRate);
  }
}

/// 计算数学运算结果 (输入为 number[], 输出为单 number)
export function computeMathResult(op: MathOp, inputs: number[]): number {
  const vals = inputs.filter((v) => typeof v === 'number' && !Number.isNaN(v));
  if (vals.length === 0) return 0;
  switch (op) {
    case 'add': return vals.reduce((a, b) => a + b, 0);
    case 'sub': return vals.reduce((a, b) => a - b, 0);
    case 'mul': return vals.reduce((a, b) => a * b, 1);
    case 'div': return vals.reduce((a, b) => (b === 0 ? 0 : a / b), vals[0] ?? 0);
    case 'avg': return vals.reduce((a, b) => a + b, 0) / vals.length;
    case 'min': return Math.min(...vals);
    case 'max': return Math.max(...vals);
    case 'abs': return Math.abs(vals[0]);
    case 'neg': return -vals[0];
    case 'square': return vals[0] * vals[0];
    case 'sqrt': return vals[0] < 0 ? 0 : Math.sqrt(vals[0]);
    case 'sin': return Math.sin(vals[0]);
    case 'cos': return Math.cos(vals[0]);
    case 'tan': return Math.tan(vals[0]);
    case 'log': return vals[0] <= 0 ? 0 : Math.log(vals[0]);
    default: return 0;
  }
}

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
  /// 派生通道数据 (Math/Filter 等节点的输出, 作为 Waveform sink 的输入)
  /// key1 = sink_widget_id, key2 = source_widget_id, value = 与 timestamps 对齐的数据
  derived?: Record<string, Record<string, number[]>>;
  /// 后端波形缓冲区当前点数
  buffer_points?: number;
  /// 后端波形缓冲区最大容量 (点)
  buffer_capacity?: number;
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

export type DataTabType = 'waveform' | 'raw' | 'pie' | 'image' | 'waveform-extra' | 'model3d' | 'spectrum' | 'command' | 'can' | 'logic' | 'frame-decoder';

export interface DataTab {
  id: string;
  type: DataTabType;
  name: string;
  widgetId?: string;
  closable: boolean;
}

// ============ 右键菜单 ============

export interface ContextMenuItem {
  id: string;
  label: string;
  icon?: ReactElement<{ size?: number; className?: string }>;
  disabled?: boolean;
  shortcut?: string;
  onClick: () => void;
}

export interface ContextMenuSeparator {
  kind: 'separator';
}

export type ContextMenuEntry = ContextMenuItem | ContextMenuSeparator;
