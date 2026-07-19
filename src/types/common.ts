import type { ReactElement } from 'react';

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

/// 频谱计算结果 — 与 Rust SpectrumResult 对应
export interface SpectrumResult {
  /// 频率 (Hz), 长度 = windowSize / 2 + 1
  frequencies: number[];
  /// 频谱值 (Magnitude/Power/PSD/Decibel), 与 frequencies 对齐
  values: number[];
}

// ============ 算术控件 ============

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

/// 单目运算集合 — 这些 op 只使用第一个输入
export const UNARY_MATH_OPS: MathOp[] = ['abs', 'neg', 'square', 'sqrt', 'sin', 'cos', 'tan', 'log'];

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

export type DataTabType = 'waveform' | 'raw' | 'pie' | 'image' | 'waveform-extra' | 'model3d' | 'spectrum' | 'command' | 'can' | 'logic' | 'frame-decoder' | 'table-view';

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
