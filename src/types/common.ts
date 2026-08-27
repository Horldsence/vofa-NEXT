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

export type RawDataDirection = 'rx' | 'tx';

/// 原始数据分片 — 与 Rust RawDataChunk 对应
export interface RawDataChunk {
  /// 微秒时间戳
  timestamp_us: number;
  /// 数据方向 — rx 接收 / tx 发送
  direction?: RawDataDirection;
  /// 数据字节 (base64 编码 — 比 JSON 数字数组省 ~2.6x 体积, atob 一次解码)
  bytes_b64: string;
}

/// 原始数据批次 — 与 Rust RawDataBatch 对应
export interface RawDataBatch {
  /// 组级单调序号 — 分片并发推送时按 seq 重组保证字节序
  seq: number;
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

/// 信号域类型 — 用于节点图端口的时域/频域/字节域静态标注
/// bytes: 字节平面端口 (Transport rx/tx, Protocol in/out, FrameDecoder in, Command loopbackOut)
export type DomainType = 'time' | 'freq' | 'bytes' | 'string';

/// FFT 求解配置 (频域求解器 — 输入时域信号 in0, 输出频谱)
export interface FFTConfig {
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

/// 频谱展示配置 (纯展示 — 从专用频谱数据通道读取某个 FFT 求解器的结果)
export interface SpectrumConfig {
  id: string;
  label: string;
  /// 数据源 FFT widget id (null = 未选择, 显示等待提示)
  sourceId: string | null;
}

/// 逆 FFT 求解配置 (频域→时域: 输入频域 spectrum, 输出时域 out0)
/// 上游 FFT 源由「频域→频域」连线在编译期解析, 无需额外配置。
export interface IFFTConfig {
  id: string;
  label: string;
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
///
/// 内部派生 (Math NodeDef `input_count` 在 widgetToNodeKind 中按 `isUnary` 强制 1);
/// 此前以 `UNARY_MATH_OPS` 公开重导出被 NodeEditor / WidgetPalette / MathWidget 共用,
/// 现在以 `isUnaryMathOp` 函数 + `lib/utils/nodeDef.ts` 内部使用替代,
/// 上层应改读 `widget.params.inputCount` (用户配置 → 后端 input_count 直接透传)。
const _UNARY_MATH_OPS: readonly MathOp[] = ['abs', 'neg', 'square', 'sqrt', 'sin', 'cos', 'tan', 'log'];

/// 单目运算判定 — 与 `_UNARY_MATH_OPS` 集合一致; 替代原 `UNARY_MATH_OPS.includes(op)`
export function isUnaryMathOp(op: MathOp): boolean {
  return _UNARY_MATH_OPS.includes(op);
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

// ============ 字符串操作控件 ============
//
// 与 Rust vofa_next_node_kind::StrOp 对应 (serde lowercase, 同 MathOp)。
// 语义规范 (以后端 str_op.rs 为准):
// - 索引 1-based (pos 从 1 开始; find 命中返回 1-based 位置, 未找到返回 0)
// - len/size = 0 表示 "到末尾/全部"
// - 字符串索引按字符计 (多字节字符安全)

/// 字符串操作种类
export type StrOp =
  | 'len'      // 长度: 字符数 (输出数值)
  | 'find'     // 查找: 子串 1-based 字符位置, 未命中 0 (输出数值)
  | 'contains' // 包含: 命中 1 / 未命中 0 (输出数值)
  | 'left'     // 左截取 size 个字符 (size=0 → 整串)
  | 'right'    // 右截取 size 个字符 (size=0 → 整串)
  | 'mid'      // 从 pos (1-based) 截取 len 个字符 (len=0 → 到末尾)
  | 'concat'   // 拼接: str1 + str2
  | 'insert'   // 在 pos (1-based) 处插入 str2
  | 'delete'   // 从 pos 删除 len 个字符 (len=0 → 删到末尾)
  | 'replace'  // 从 pos 起将 len 个字符替换为 str2
  | 'upper'    // 转大写
  | 'lower'    // 转小写
  | 'trim'     // 去除首尾空白
  | 'reverse'; // 按字符反转

/// 字符串操作端口描述
export interface StrOpPort {
  id: string;
  label: string;
  domain: DomainType;
}

/// 单个字符串操作的端口表 — 与 Rust StrOp::input_ports / output_domain 完全一致 (唯一事实源)
export interface StrOpMeta {
  /// 输入端口 (固定顺序, 后端 evaluate 依此取参)
  inputs: StrOpPort[];
  /// 输出端口 (固定 id "result") 的域: len/find/contains 为数值 (time), 其余为字符串
  outputDomain: DomainType;
  /// 带内联数值输入框的数值端口 id — StrWidget 据此渲染内联框;
  /// 端口未连接时回退到 StrConfig 的 pos/len/size 同名字段
  inlineNumPorts: string[];
}

// 端口组常量 (复用以避免重复书写)
const STR_IN_STR: StrOpPort[] = [{ id: 'str', label: 'str', domain: 'string' }];
const STR_IN_STR_SUBSTR: StrOpPort[] = [
  { id: 'str', label: 'str', domain: 'string' },
  { id: 'substr', label: 'substr', domain: 'string' },
];
const STR_IN_STR_SIZE: StrOpPort[] = [
  { id: 'str', label: 'str', domain: 'string' },
  { id: 'size', label: 'size', domain: 'time' },
];
const STR_IN_STR_POS_LEN: StrOpPort[] = [
  { id: 'str', label: 'str', domain: 'string' },
  { id: 'pos', label: 'pos', domain: 'time' },
  { id: 'len', label: 'len', domain: 'time' },
];
const STR_IN_STR1_STR2: StrOpPort[] = [
  { id: 'str1', label: 'str1', domain: 'string' },
  { id: 'str2', label: 'str2', domain: 'string' },
];
const STR_IN_STR1_STR2_POS: StrOpPort[] = [
  { id: 'str1', label: 'str1', domain: 'string' },
  { id: 'str2', label: 'str2', domain: 'string' },
  { id: 'pos', label: 'pos', domain: 'time' },
];
const STR_IN_STR1_STR2_POS_LEN: StrOpPort[] = [
  { id: 'str1', label: 'str1', domain: 'string' },
  { id: 'str2', label: 'str2', domain: 'string' },
  { id: 'pos', label: 'pos', domain: 'time' },
  { id: 'len', label: 'len', domain: 'time' },
];

/// 全部字符串操作的端口元数据表
///
/// 后端 `node_kind::StrOp::input_ports()` 与 `output_domain()` 是权威定义。
/// 此处 TS 镜像仅供前端 UI 渲染节点把手 / 内联数值框使用; 输出域部分 (`outputDomain`)
/// 在 StrWidget 中后续可改读 `derivedPorts` (后端 graph:derived 事件); 输入把手
/// (`inputs`) 因 backend derived_ports 仅枚举输出端口, 暂时仍需本常量作为 UI 占位输入。
export const STR_OP_PORTS: Record<StrOp, StrOpMeta> = {
  len: { inputs: STR_IN_STR, outputDomain: 'time', inlineNumPorts: [] },
  find: { inputs: STR_IN_STR_SUBSTR, outputDomain: 'time', inlineNumPorts: [] },
  contains: { inputs: STR_IN_STR_SUBSTR, outputDomain: 'time', inlineNumPorts: [] },
  left: { inputs: STR_IN_STR_SIZE, outputDomain: 'string', inlineNumPorts: ['size'] },
  right: { inputs: STR_IN_STR_SIZE, outputDomain: 'string', inlineNumPorts: ['size'] },
  mid: { inputs: STR_IN_STR_POS_LEN, outputDomain: 'string', inlineNumPorts: ['pos', 'len'] },
  concat: { inputs: STR_IN_STR1_STR2, outputDomain: 'string', inlineNumPorts: [] },
  insert: { inputs: STR_IN_STR1_STR2_POS, outputDomain: 'string', inlineNumPorts: ['pos'] },
  delete: { inputs: STR_IN_STR_POS_LEN, outputDomain: 'string', inlineNumPorts: ['pos', 'len'] },
  replace: { inputs: STR_IN_STR1_STR2_POS_LEN, outputDomain: 'string', inlineNumPorts: ['pos', 'len'] },
  upper: { inputs: STR_IN_STR, outputDomain: 'string', inlineNumPorts: [] },
  lower: { inputs: STR_IN_STR, outputDomain: 'string', inlineNumPorts: [] },
  trim: { inputs: STR_IN_STR, outputDomain: 'string', inlineNumPorts: [] },
  reverse: { inputs: STR_IN_STR, outputDomain: 'string', inlineNumPorts: [] },
};

// ============ 3D 模型显示 ============

/// 3D 显示模式
/// - trajectory:           xyz 作为位置, 渲染拖尾轨迹
/// - attitude:             roll/pitch/yaw 作为欧拉角 (弧度), 渲染旋转模型
/// - trajectory-attitude:  同时显示拖尾轨迹 + 跟随位置/姿态旋转的模型
export type Model3DMode = 'trajectory' | 'attitude' | 'trajectory-attitude';

/// 3D 模型源
/// - builtin-cube: 默认半透明立方体 (向后兼容旧 widget)
/// - custom:       用户通过 Tauri 对话框导入的 GLB/GLTF, path 持久化到 widget 配置
export type Model3DSource =
  | { kind: 'builtin-cube' }
  | { kind: 'custom'; path: string; name: string };

/// 3D 模型控件配置
/// 输入端口: x / y / z (位置) + roll / pitch / yaw (旋转, 缺失补 0)
/// 各模式实际使用的端口:
///   trajectory           -> x / y / z
///   attitude             -> roll / pitch / yaw
///   trajectory-attitude  -> x / y / z + roll / pitch / yaw
export interface Model3DConfig {
  id: string;
  label: string;
  /// 显示模式
  mode: Model3DMode;
  /// 拖尾长度 (trajectory / trajectory-attitude 模式, 默认 200)
  trailLength: number;
  /// 拖尾/立方体颜色 (HEX, 如 '#75beff')
  color: string;
  /// 坐标轴长度 (默认 1.0)
  axisLength: number;
  /// 模型来源 (默认 builtin-cube)
  modelSource: Model3DSource;
}

// ============ Biquad 滤波器系数 (后端单一权威) ============
//
// 系数派生由后端 `dsp_filter::filter_kind_from_config` 承担 — 前端 widget.params
// (preset + cutoff/low/high + sample_rate) 原样下发, 后端编译/求值时
// 按 RBJ Audio EQ Cookbook 在 `dsp_filter::lowpass_biquad` 等派生 [b, a]。
// 不再需要前端 b/a 计算; 公式参考:
// https://www.musicdsp.org/en/latest/Filters/197-rbj-audio-eq-cookbook.html

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

export type DataTabType = 'waveform' | 'raw' | 'pie' | 'image' | 'waveform-extra' | 'model3d' | 'spectrum' | 'command' | 'can' | 'logic' | 'frame-decoder' | 'table-view' | 'trigger' | 'compile-errors' | 'compile-results' | 'operation-history';

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
