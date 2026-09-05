// ============ 控件配置 ============

import type {
  MathConfig, FilterConfig, FFTConfig, IFFTConfig, SpectrumConfig, Model3DConfig, WidgetBinding, StrOp,
} from './common';
import type { CommandConfig, FrameDecoderConfig, TableViewConfig } from './frameDecoder';

export interface WidgetBaseConfig {
  id: string;
  label: string;
}

export interface ChoiceOption {
  id: string;
  label: string;
  value: number;
}

// ============ 显示量程/刻度 ============

/// 显示控件的量程/刻度配置 (Gauge / Progress 共用)
/// - manual: 直接使用 min/max, 刻度均分;
/// - auto:   滑动窗口 (windowSec) 自适应, 1-2-5 取整; 无样本时回退 min/max;
///   min/max 同时作为「捕获自动量程」的落点。
export interface DisplayRangeConfig {
  mode: 'manual' | 'auto';
  min: number;
  max: number;
  /// auto 模式滑动窗口 (秒), 默认 10
  windowSec: number;
  /// 主刻度标签数 (含两端), 默认 5, 钳制 2..11
  majorTicks: number;
  /// 刻度/读数小数位; 'auto' = 由刻度间距推导
  precision: 'auto' | number;
}

/// 显示量程默认值 (创建控件与归一化兜底共用)
export const DEFAULT_DISPLAY_RANGE: DisplayRangeConfig = {
  mode: 'manual',
  min: 0,
  max: 100,
  windowSec: 10,
  majorTicks: 5,
  precision: 'auto',
};

export interface KnobConfig extends WidgetBaseConfig {
  min: number;
  max: number;
  step: number;
  value: number;
  binding: WidgetBinding;
}

export interface ButtonConfig extends WidgetBaseConfig {
  pressValue: number;
  releaseValue: number;
  binding: WidgetBinding;
}

export interface RadioConfig extends WidgetBaseConfig {
  options: ChoiceOption[];
  selectedId: string;
  binding: WidgetBinding;
}

export interface CheckboxConfig extends WidgetBaseConfig {
  options: ChoiceOption[];
  selectedIds: string[];
  /** 旧版单复选框的未选中返回值；新控件固定为 0。 */
  emptyValue?: number;
  binding: WidgetBinding;
}

export interface SliderConfig extends WidgetBaseConfig {
  min: number;
  max: number;
  step: number;
  value: number;
  binding: WidgetBinding;
}

export interface LabelConfig extends WidgetBaseConfig {
  text: string;
  /** @deprecated 旧工作区兼容；新节点通过 value 输入边绑定。 */
  channel: number | null;
}

export interface WaveformConfig extends WidgetBaseConfig {
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
  /** @deprecated 旧工作区兼容；新节点通过 segN 输入边绑定。 */
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
  /// 量程/刻度配置 (手动或滑动窗口自适应)
  range: DisplayRangeConfig;
  unit: string;          // 单位后缀, 如 'V' / 'A' / ''
  /** @deprecated 旧工作区兼容；新节点通过 value 输入边绑定。 */
  channel: number | null; // 绑定的输入通道 (null = 不绑定)
}

/// 进度条控件 — 横向/纵向条形显示单通道实时值占比
export interface ProgressConfig {
  id: string;
  label: string;
  /// 量程/刻度配置 (手动或滑动窗口自适应)
  range: DisplayRangeConfig;
  unit: string;
  orientation: 'horizontal' | 'vertical';
  /// 是否在条旁显示当前数值
  showValue: boolean;
  /// 填充色 HEX; '' = 主题默认色
  color: string;
  /** @deprecated 旧工作区兼容；新节点通过 value 输入边绑定。 */
  channel: number | null;
}

/// LED 指示灯 — 阈值控制开关色
export interface LEDConfig {
  id: string;
  label: string;
  threshold: number;     // 输入 >= threshold 视为 ON
  on_color: string;      // HEX, 如 '#89d185'
  off_color: string;     // HEX, 如 '#3c3c3c'
  /** @deprecated 旧工作区兼容；新节点通过 value 输入边绑定。 */
  channel: number | null;
}

/// 大数字显示 — 大字号展示单通道数值
export interface NumberDisplayConfig {
  id: string;
  label: string;
  unit: string;
  precision: number;     // 小数位数
  /** @deprecated 旧工作区兼容；新节点通过 value 输入边绑定。 */
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

/// 原始字节流查看控件 — 数据 Tab 渲染全局原始字节流 (RawDataView)
export interface RawDataConfig {
  id: string;
  label: string;
  /// 该卡片选中的输入端口 key (`src:<sourceId>:<sourceHandle>`, 见 rawDataPortId 约定)。
  /// 每张 RawData 卡片独立选择, 互不共用; 仅在该连线存在时生效, 失效/缺省回退第一个已连接端口。
  /// 可选: 旧保存数据无此字段 (serde/TS 均按缺省处理)
  selectedInput?: string;
}

/// 文本展示控件 — 接收字符串端口输入, 节点内直接渲染
export interface TextDisplayConfig {
  id: string;
  label: string;
  fontSize: 'sm' | 'base' | 'lg';
  monospace: boolean;
}

/// 文本输入控件 — 节点内文本框, 内容作为参数 text 经 update_tab_graph 同步到后端,
/// 后端每帧原样写入字符串平面 out_str[id]["str"] (唯一输出端口 str, string 域)
export interface TextInputConfig {
  id: string;
  label: string;
  text: string;
  placeholder: string;
}

/// 字符串操作控件 — 对字符串输入做截取/拼接/替换/格式化等操作
/// 端口表见 STR_OP_PORTS (与后端 StrOp::input_ports 一致);
/// pos/len/size 为对应数值端口未连接时的内联回退值 (已连接则由后端用上游值)
export interface StrConfig {
  id: string;
  label: string;
  op: StrOp;
  /// pos 内联回退值 (默认 1, 1-based 起点)
  pos: number;
  /// len 内联回退值 (默认 0 = 到末尾)
  len: number;
  /// size 内联回退值 (默认 0 = 全部)
  size: number;
  /// FORMAT 模板文本 — "fmt" 端口未连接时的内联回退 ({N} 引用第 N 路, {N:.P} 定精度);
  /// 仅 op === 'format' 使用。可选: 旧保存数据无此字段 (后端 serde default 兜底空串)
  tmpl?: string;
}

/// 触发器匹配类型 — 与后端 TriggerMatchType 对齐
export type TriggerMatchType = 'exact' | 'prefix' | 'contains' | 'regex' | 'range' | 'glob';

/// 触发器单条规则
export interface TriggerRule {
  id: string;                        // nanoid(6), 前端 React key 用
  pattern: string;                   // 命令模板 (字符串 / 正则 / 范围 / glob)
  matchType: TriggerMatchType;
  /// 输出值类型: 'number' (写入 value 端口) | 'string' (写入 text 端口)
  outputType: 'number' | 'string';
  /// 数字输出值 (outputType='number' 时使用)
  outputValue: number;
  /// 字符串输出值 (outputType='string' 时使用)
  outputText: string;
  flags?: string;                    // 正则 flags, 如 'i' / 'im'
  enabled: boolean;
}

/// 触发器匹配结果 — 与后端 TriggerMatchResult 对齐
export interface TriggerMatchResult {
  value: number;
  matched: boolean;
  text: string;
  outputType: 'number' | 'string' | 'miss';
}

/// 触发器控件 — 命令字符串 → 输出通道数据
///
/// 模式 (manual / auto) 与 FrameDecoder 同构:
/// - manual: 面板内文本框编辑 command, 后端每帧以当前 command 匹配
/// - auto:   上游 trigger 端口 (number) 按 edge (level/rising) 由后端边沿检测驱动
///
/// 匹配由后端图求值 (Rust TriggerMatcher, 见 node_engine evaluate.rs):
/// 结果按规则的 `outputType` 写入 `value` / `matched` (f32) 或 `text` (string) 端口
/// 供下游消费; 前端只读 store 快照展示, 不再调用 match_trigger_command 驱动。
export interface TriggerConfig {
  id: string;
  label: string;
  mode: 'manual' | 'auto';
  edge: 'level' | 'rising';          // 仅 mode==='auto' 时生效
  defaultMiss: number;               // 全部规则未命中时 value 端口的默认值
  defaultMissText: string;           // 全部规则未命中时 text 端口的默认值
  command: string;                   // 当前待匹配命令 (manual: 面板输入; auto: 节点文本框)
  rules: TriggerRule[];
  binding?: WidgetBinding;           // 与其它输入控件一致 (暂未使用, 保留)
}

// ============ 控件类别 ============

/// 控件类别 — 用于 WidgetPalette 分组与颜色区分
export type WidgetCategory =
  | 'transport'
  | 'protocol'
  | 'send'
  | 'input'      // 数据类 (Knob/Button/Radio/Checkbox/Slider/Command)
  | 'display'    // 显示控件 (Waveform/PieChart/Image/Gauge/LED/NumberDisplay/Label/Spectrum/Model3D)
  | 'math'       // 算术控件 (Math/Filter — 加减乘除/数学函数/滤波)
  | 'string'     // 字符串控件 (Str — 字符串截取/拼接/查找/大小写转换)
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
  | { kind: 'Progress'; params: ProgressConfig }
  | { kind: 'LED'; params: LEDConfig }
  | { kind: 'NumberDisplay'; params: NumberDisplayConfig }
  | { kind: 'Custom'; params: CustomConfig }
  | { kind: 'Math'; params: MathConfig }
  | { kind: 'Filter'; params: FilterConfig }
  | { kind: 'FFT'; params: FFTConfig }
  | { kind: 'IFFT'; params: IFFTConfig }
  | { kind: 'Spectrum'; params: SpectrumConfig }
  | { kind: 'Model3D'; params: Model3DConfig }
  | { kind: 'Command'; params: CommandConfig }
  | { kind: 'FrameDecoder'; params: FrameDecoderConfig }
  | { kind: 'TableView'; params: TableViewConfig }
  | { kind: 'RawData'; params: RawDataConfig }
  | { kind: 'Trigger'; params: TriggerConfig }
  | { kind: 'TextDisplay'; params: TextDisplayConfig }
  | { kind: 'TextInput'; params: TextInputConfig }
  | { kind: 'Str'; params: StrConfig }
  | { kind: 'TextOut'; params: TextOutConfig };

/// 文本下发控件 — 动态发送回传: 把图内字符串 (text 输入口) 写回目标 Transport 的 tx
/// 后端求值经通用字符串发布进入 graph_string_outputs, 由发送 ticker 按 minIntervalMs
/// 限速发送; Send 按钮走 send_text_out_now 强制立即发送一次
export interface TextOutConfig {
  id: string;
  label: string;
  /// 目标 Transport 全局节点 id ('' = 未选择, 不发送)
  targetTransport: string;
  /// 发送时附加的换行后缀
  newline: 'none' | 'lf' | 'crlf' | 'cr';
  /// 自动发送最小间隔 ms (值变化限速)
  minIntervalMs: number;
}

/// 获取控件所属类别 (用于 palette 分组与着色)
export function getWidgetCategory(kind: WidgetConfig['kind']): WidgetCategory {
  switch (kind) {
    case 'Knob':
    case 'Button':
    case 'Radio':
    case 'Checkbox':
    case 'Slider':
      return 'input';
    case 'Command':
      return 'send';
    case 'Waveform':
    case 'PieChart':
    case 'Image':
    case 'Gauge':
    case 'Progress':
    case 'LED':
    case 'NumberDisplay':
    case 'Label':
    case 'Spectrum':
    case 'Model3D':
    case 'TableView':
    case 'RawData':
      return 'display';
    case 'Math':
    case 'Filter':
    case 'FFT':
    case 'IFFT':
      return 'math';
    case 'Custom':
      return 'custom';
    case 'FrameDecoder':
      return 'protocol';
    case 'Trigger':
      return 'input';
    case 'TextInput':
      return 'input';
    case 'TextDisplay':
      return 'display';
    case 'Str':
      return 'string';
    case 'TextOut':
      return 'send';
  }
}

/// 各类别主题色 — WidgetPalette 分组着色与节点卡片着色共用
export const WIDGET_CATEGORY_COLORS: Record<WidgetCategory, string> = {
  transport: '#dcdcaa',
  protocol: '#4ec9b0',
  send: '#c586c0',
  input: '#4fc3f7',
  display: '#81c784',
  math: '#ffb74d',
  string: '#ff8a65',
  custom: '#ba68c8',
};
