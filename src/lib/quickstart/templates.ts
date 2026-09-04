//! 快速开始模板 — 内置节点图 + 窗口组织 + 传输/协议配置
//!
//! 每个模板返回一份「类快照」(AppSnapshot), 应用时:
//!   - 替换: 清空工作区后按分区应用 (不含设置)
//!   - 合并: 作为新控件标签页追加 (见 applyTemplate.ts 的 ID 重映射)
//!
//! 内置模板: 数学 / 滤波器 / 频谱分析 / CAN / 串口 / 综合演示 (TestData, 无硬件可跑)。

import { type Node, type Edge } from '@xyflow/react';
import { createWidget } from '../utils/createWidget';
import { WIDGET_DEFAULT_WIDTH } from '../utils/widgetSize';
import { ALL_BACKUP_SECTIONS, type AppSnapshot } from '../tauri/appExport';
import { schemaFromProtocolConfig } from '../utils/protocolSchema';
import { rawDataPortId } from '../utils/nodeDef';
import type { DockNode, DockCard } from '../../store/dockStore';
import type { WidgetConfig, DataTab, ControlTab, TransportConfig, ProtocolConfig, ProtocolSchema } from '../../types';

const TAB = 'default';
/// 模板内全局节点 id (应用时由 applySnapshot/mergeTemplate 处理)
const TRANSPORT_ID = 'template-transport';
const PROTOCOL_ID = 'template-protocol';

// ==================== 构建辅助 ====================

/// 以指定 id/label 创建控件, 覆盖默认随机 id 与 label
function widget(
  kind: WidgetConfig['kind'],
  id: string,
  label: string,
  patch?: Record<string, unknown>
): WidgetConfig {
  const w = createWidget(kind);
  const params = w.params as unknown as Record<string, unknown>;
  params.id = id;
  if ('label' in params) params.label = label;
  if (patch) Object.assign(params, patch);
  return w;
}

/// 控件节点
function wnode(id: string, w: WidgetConfig, x: number, y: number): Node {
  return { id, type: 'widget', position: { x, y }, width: WIDGET_DEFAULT_WIDTH, data: { widget: w, tabId: TAB } };
}

/// 边
function edge(
  id: string,
  source: string,
  sourceHandle: string,
  target: string,
  targetHandle: string
): Edge {
  return { id, source, sourceHandle, target, targetHandle };
}

/// 全局 Transport / Protocol 节点 (内联实现, 避免引入 appStoreHelpers 造成循环依赖)
function transportNode(config: TransportConfig): Node {
  return {
    id: TRANSPORT_ID,
    type: 'transport',
    position: { x: 40, y: 40 },
    data: { global: true, config, label: config.kind },
  };
}

/// 全局 Protocol 节点 — schema 模型: 预设经 schemaFromProtocolConfig 生成,
/// 自定义模板可经 schema 参数传 preset='custom' 的帧定义
function protocolNode(config: ProtocolConfig, channels = 4, schema?: ProtocolSchema): Node {
  return {
    id: PROTOCOL_ID,
    type: 'protocol',
    position: { x: 260, y: 40 },
    data: {
      global: true,
      config,
      convertTo: null,
      channels,
      schema: schema ?? schemaFromProtocolConfig(config),
      label: schema?.preset === 'custom' ? 'Custom' : config.kind,
    },
  };
}

/// 固定波形数据 Tab (始终存在, 不可关闭)
const WAVEFORM_FIXED: DataTab = {
  id: 'waveform-fixed',
  type: 'waveform',
  name: 'Waveform',
  closable: false,
};

/// 节点属性面板 Tab — 默认布局的专属卡承载 (与 dockStore defaultRoot 同形)
const NODE_PROPERTIES_FIXED: DataTab = {
  id: 'node-properties-fixed',
  type: 'node-properties',
  name: 'Properties',
  closable: true,
};

/// 部分控件会派生一个数据 Tab (Waveform/Spectrum/RawData)
function derivedDataTab(w: WidgetConfig): DataTab | null {
  switch (w.kind) {
    case 'Waveform':
      return { id: w.params.id, type: 'waveform-extra', name: 'Waveform', widgetId: w.params.id, closable: true };
    case 'Spectrum':
      return { id: w.params.id, type: 'spectrum', name: w.params.label, widgetId: w.params.id, closable: true };
    case 'RawData':
      return { id: w.params.id, type: 'raw', name: w.params.label, widgetId: w.params.id, closable: true };
    default:
      return null;
  }
}

/// 默认 Dock 布局: 上方左侧画布 | 右侧属性面板 (较小), 下方数据面板
/// (与 dockStore defaultRoot 同形 — 两处需保持一致)
function buildDock(
  controlTabIds: string[],
  dataTabIds: string[],
  activeDataTabId: string
): { dockRoot: DockNode; dockCards: Record<string, DockCard> } {
  const cards: Record<string, DockCard> = {
    'control-main': {
      id: 'control-main',
      kind: 'control',
      tabIds: controlTabIds,
      activeTabId: controlTabIds[0] ?? null,
    },
    // data-main 先于 properties-main — reconcile 安置无家可归 tab 时取「第一张
    // 该 kind 卡片」(插入序), 属性专属卡不应收容其他数据 tab
    'data-main': {
      id: 'data-main',
      kind: 'data',
      tabIds: dataTabIds,
      activeTabId: activeDataTabId ?? dataTabIds[0] ?? null,
    },
    'properties-main': {
      id: 'properties-main',
      kind: 'data',
      tabIds: ['node-properties-fixed'],
      activeTabId: 'node-properties-fixed',
    },
  };
  const root: DockNode = {
    id: 'split-root',
    type: 'split',
    dir: 'col',
    children: [
      {
        id: 'split-top',
        type: 'split',
        dir: 'row',
        children: [
          { id: 'node-control', type: 'card', cardId: 'control-main' },
          { id: 'node-props', type: 'card', cardId: 'properties-main' },
        ],
        sizes: [72, 28],
      },
      { id: 'node-data', type: 'card', cardId: 'data-main' },
    ],
    sizes: [55, 45],
  };
  return { dockRoot: root, dockCards: cards };
}

/// 模板默认聚焦的数据 Tab — 应用模板后首先看向「编译结果」,核对连线质量
/// (compile-errors / compile-results 是 appStore 初始 dataTabs 里固定存在的两个 Tab)
const TEMPLATE_DEFAULT_ACTIVE_DATA_TAB = 'compile-results-fixed';

/// 组装快照
function buildSnapshot(opts: {
  name: string;
  protocol: ProtocolConfig;
  transport: TransportConfig;
  widgetNodes: Node[];
  edges: Edge[];
  extraDataTabs?: DataTab[];
  sourceChannels?: number;
  /// 自定义协议 schema (preset='custom'); 缺省由 protocol 配置工厂生成预设 schema
  protocolSchema?: ProtocolSchema;
}): AppSnapshot {
  const widgets = opts.widgetNodes.map((n) => n.data.widget as WidgetConfig);
  const controlTabs: ControlTab[] = [
    { id: TAB, name: opts.name, widgets: opts.widgetNodes.map((n) => n.id) },
  ];
  const derivedTabs: DataTab[] = [];
  for (const w of widgets) {
    const t = derivedDataTab(w);
    if (t && !derivedTabs.some((x) => x.id === t.id)) derivedTabs.push(t);
  }
  const extraTabs = opts.extraDataTabs ?? [];
  const dataTabs: DataTab[] = [WAVEFORM_FIXED, NODE_PROPERTIES_FIXED, ...derivedTabs, ...extraTabs];
  const activeDataTabId = TEMPLATE_DEFAULT_ACTIVE_DATA_TAB;
  const { dockRoot, dockCards } = buildDock(
    [TAB],
    // 属性面板 tab 由专属卡 (properties-main) 承载, 不进数据卡
    dataTabs.map((t) => t.id).filter((id) => id !== NODE_PROPERTIES_FIXED.id),
    activeDataTabId
  );

  return {
    version: 3,
    exportedAt: new Date().toISOString(),
    sections: ALL_BACKUP_SECTIONS,
    // 模板不含设置 — 应用时保留用户当前设置
    widgets,
    controlTabs,
    dataTabs,
    activeDataTabId,
    activeControlTabId: TAB,
    rfNodes: [
      transportNode(opts.transport),
      protocolNode(opts.protocol, opts.sourceChannels ?? 4, opts.protocolSchema),
      ...opts.widgetNodes,
    ],
    rfEdges: [
      // Transport.rx → Protocol.in 字节边
      {
        id: 'edge-transport-protocol',
        source: TRANSPORT_ID,
        sourceHandle: 'rx',
        target: PROTOCOL_ID,
        targetHandle: 'in',
      },
      ...opts.edges,
    ],
    rawDataViewPrefs: {},
    dockRoot,
    dockCards,
    sidebarDock: 'left',
  };
}

// ==================== 模板定义 ====================

export interface QuickStartTemplate {
  id: string;
  nameKey: string;
  descKey: string;
  build: () => AppSnapshot;
}

function mathTemplate(): AppSnapshot {
  const mSin = widget('Math', 'm-sin', 'sin', { op: 'sin', inputCount: 1 });
  const ndSin = widget('NumberDisplay', 'nd-sin', 'sin(ch0)');
  const mAdd = widget('Math', 'm-add', 'sum', { op: 'add', inputCount: 2 });
  const ndAdd = widget('NumberDisplay', 'nd-add', 'ch0+ch1');
  return buildSnapshot({
    name: 'Math',
    protocol: { kind: 'JustFloat', channels: 4 },
    transport: { kind: 'TestData', params: { channels: 4, sample_rate: 1000, signal: 'Sine' } },
    widgetNodes: [
      wnode('m-sin', mSin, 300, 80),
      wnode('nd-sin', ndSin, 560, 80),
      wnode('m-add', mAdd, 300, 260),
      wnode('nd-add', ndAdd, 560, 260),
    ],
    edges: [
      edge('e1', PROTOCOL_ID, 'ch0', 'm-sin', 'in0'),
      edge('e2', 'm-sin', 'result', 'nd-sin', 'value'),
      edge('e3', PROTOCOL_ID, 'ch0', 'm-add', 'in0'),
      edge('e4', PROTOCOL_ID, 'ch1', 'm-add', 'in1'),
      edge('e5', 'm-add', 'result', 'nd-add', 'value'),
    ],
  });
}

function filterTemplate(): AppSnapshot {
  const fLp = widget('Filter', 'f-lp', 'Lowpass', { preset: 'Lowpass', cutoff: 100, sampleRate: 1000 });
  const ndLp = widget('NumberDisplay', 'nd-lp', 'filtered');
  const wf = widget('Waveform', 'wf-filter', 'Filtered', { channels: 4, dynamicSeries: true });
  return buildSnapshot({
    name: 'Filter',
    protocol: { kind: 'JustFloat', channels: 4 },
    transport: { kind: 'TestData', params: { channels: 4, sample_rate: 1000, signal: 'MultiTone' } },
    widgetNodes: [
      wnode('f-lp', fLp, 300, 80),
      wnode('nd-lp', ndLp, 560, 80),
      wnode('wf-filter', wf, 300, 260),
    ],
    edges: [
      edge('e1', PROTOCOL_ID, 'ch0', 'f-lp', 'in0'),
      edge('e2', 'f-lp', 'result', 'nd-lp', 'value'),
      edge('e3', PROTOCOL_ID, 'ch0', 'wf-filter', 'CH0'),
      edge('e4', 'f-lp', 'result', 'wf-filter', 'CH1'),
    ],
  });
}

function canTemplate(): AppSnapshot {
  return buildSnapshot({
    name: 'CAN',
    protocol: { kind: 'Slcan' },
    transport: {
      kind: 'Slcan',
      params: { port_name: '', baud_rate: 115200, can_bitrate: 'Bps500k' },
    },
    widgetNodes: [],
    edges: [],
    extraDataTabs: [{ id: 'can-main', type: 'can', name: 'CAN', closable: true }],
  });
}

function serialTemplate(): AppSnapshot {
  const g0 = widget('Gauge', 'g-ch0', 'CH0');
  const g1 = widget('Gauge', 'g-ch1', 'CH1');
  const n2 = widget('NumberDisplay', 'nd-ch2', 'CH2');
  const n3 = widget('NumberDisplay', 'nd-ch3', 'CH3');
  return buildSnapshot({
    name: 'Serial',
    protocol: { kind: 'JustFloat', channels: null },
    transport: {
      kind: 'Serial',
      params: {
        port_name: '',
        baud_rate: 115200,
        data_bits: 8,
        parity: 'none',
        stop_bits: 'one',
        flow_control: 'none',
      },
    },
    widgetNodes: [
      wnode('g-ch0', g0, 300, 60),
      wnode('g-ch1', g1, 560, 60),
      wnode('nd-ch2', n2, 300, 260),
      wnode('nd-ch3', n3, 560, 260),
    ],
    edges: [
      edge('e1', PROTOCOL_ID, 'ch0', 'g-ch0', 'value'),
      edge('e2', PROTOCOL_ID, 'ch1', 'g-ch1', 'value'),
      edge('e3', PROTOCOL_ID, 'ch2', 'nd-ch2', 'value'),
      edge('e4', PROTOCOL_ID, 'ch3', 'nd-ch3', 'value'),
    ],
  });
}

function demoTemplate(): AppSnapshot {
  const fLp = widget('Filter', 'f-lp', 'Lowpass', { preset: 'Lowpass', cutoff: 50, sampleRate: 1000 });
  const ndLp = widget('NumberDisplay', 'nd-lp', 'filtered');
  const mSin = widget('Math', 'm-sin', 'sin', { op: 'sin', inputCount: 1 });
  const gSin = widget('Gauge', 'g-sin', 'sin(ch1)');
  const wf = widget('Waveform', 'wf', 'Filtered', { channels: 4, dynamicSeries: true });
  return buildSnapshot({
    name: 'Demo',
    protocol: { kind: 'JustFloat', channels: 4 },
    transport: { kind: 'TestData', params: { channels: 4, sample_rate: 1000, signal: 'Sine' } },
    widgetNodes: [
      wnode('f-lp', fLp, 300, 60),
      wnode('nd-lp', ndLp, 560, 60),
      wnode('m-sin', mSin, 300, 220),
      wnode('g-sin', gSin, 560, 220),
      wnode('wf', wf, 300, 400),
    ],
    edges: [
      edge('e1', PROTOCOL_ID, 'ch0', 'f-lp', 'in0'),
      edge('e2', 'f-lp', 'result', 'nd-lp', 'value'),
      edge('e3', 'f-lp', 'result', 'wf', 'CH0'),
      edge('e4', PROTOCOL_ID, 'ch1', 'm-sin', 'in0'),
      edge('e5', 'm-sin', 'result', 'g-sin', 'value'),
    ],
  });
}

function fftTemplate(): AppSnapshot {
  // 频域流水线: ch0 → FFT 求解器 → 频谱仪 (连线即数据源) → IFFT 重建回时域 → 波形对比
  const fft = widget('FFT', 'fft-main', 'FFT', {
    windowSize: 512,
    windowType: 'Hann',
    output: 'Magnitude',
    sampleRate: 1000,
  });
  const spec = widget('Spectrum', 'spec-main', 'Spectrum');
  const ifft = widget('IFFT', 'ifft-main', 'IFFT');
  const wf = widget('Waveform', 'wf-ifft', 'Original vs IFFT', { channels: 4, dynamicSeries: true });
  return buildSnapshot({
    name: 'FFT',
    protocol: { kind: 'JustFloat', channels: 4 },
    transport: { kind: 'TestData', params: { channels: 4, sample_rate: 1000, signal: 'MultiTone' } },
    widgetNodes: [
      wnode('fft-main', fft, 300, 60),
      wnode('spec-main', spec, 600, 60),
      wnode('wf-ifft', wf, 300, 280),
      wnode('ifft-main', ifft, 600, 280),
    ],
    edges: [
      edge('e1', PROTOCOL_ID, 'ch0', 'fft-main', 'in0'),
      edge('e2', 'fft-main', 'spectrum', 'spec-main', 'spectrum'),
      edge('e3', 'fft-main', 'spectrum', 'ifft-main', 'spectrum'),
      edge('e4', PROTOCOL_ID, 'ch0', 'wf-ifft', 'CH0'),
      edge('e5', 'ifft-main', 'out0', 'wf-ifft', 'CH1'),
    ],
  });
}

/// 自定义协议演示: custom 块协议 (命名端口) + 多帧 CommandSender
/// 帧布局与 TestData 的 JustFloat 线缆格式一致 (2×float32LE + 帧尾),
/// 因此连接 TestData 即可看到 speed/temp 命名通道数据
function customProtocolTemplate(): AppSnapshot {
  const customSchema: ProtocolSchema = {
    preset: 'custom',
    decode: [
      { id: 'cp-f0', type: 'field', fieldType: 'float32LE', portName: 'speed' },
      { id: 'cp-f1', type: 'field', fieldType: 'float32LE', portName: 'temp' },
      { id: 'cp-tail', type: 'tail', hex: '00 00 80 7F' },
    ],
  };
  const cmd = widget('Command', 'cmd-main', 'Command', {
    frames: [
      {
        id: 'cmd-main-frame-1',
        label: 'Ping',
        blocks: [
          { id: 'b1', type: 'const_hex', hex: 'AA 01' },
          { id: 'b2', type: 'checksum', checksum: 'sum8' },
        ],
        appendNewline: false,
        sendMode: 'manual',
        timerMs: 1000,
      },
      {
        id: 'cmd-main-frame-2',
        label: 'Set Speed',
        blocks: [
          { id: 'b3', type: 'const_hex', hex: 'BB' },
          { id: 'b4', type: 'var_ref', portName: 'speed', fieldType: 'float32LE' },
        ],
        appendNewline: false,
        sendMode: 'manual',
        timerMs: 1000,
      },
    ],
  });
  const wf = widget('Waveform', 'wf-custom', 'Speed', { channels: 4, dynamicSeries: true });
  const gTemp = widget('Gauge', 'g-temp', 'Temp');
  const raw = widget('RawData', 'raw-custom', 'RawData');
  return buildSnapshot({
    name: 'Custom Protocol',
    // config 保留 JustFloat(2ch): TestData 按 legacy 格式生成字节流, custom decode 负责解析
    protocol: { kind: 'JustFloat', channels: 2 },
    protocolSchema: customSchema,
    sourceChannels: 2,
    transport: { kind: 'TestData', params: { channels: 2, sample_rate: 1000, signal: 'Sine' } },
    widgetNodes: [
      wnode('wf-custom', wf, 560, 60),
      wnode('g-temp', gTemp, 560, 260),
      wnode('cmd-main', cmd, 260, 260),
      wnode('raw-custom', raw, 820, 160),
    ],
    edges: [
      // 命名端口 → 波形/仪表 (schema 派生端口 speed/temp)
      edge('e1', PROTOCOL_ID, 'speed', 'wf-custom', 'CH0'),
      edge('e2', PROTOCOL_ID, 'temp', 'g-temp', 'value'),
      // 协议字节出口 → RawData (动态端口 src:<id>:out)
      edge('e3', PROTOCOL_ID, 'out', 'raw-custom', rawDataPortId(PROTOCOL_ID, 'out')),
      // 多帧发送器 → 设备写入口
      edge('e4', 'cmd-main', 'loopbackOut', TRANSPORT_ID, 'tx'),
    ],
  });
}

export const QUICK_START_TEMPLATES: QuickStartTemplate[] = [
  { id: 'math', nameKey: 'templateMath', descKey: 'templateMathDesc', build: mathTemplate },
  { id: 'filter', nameKey: 'templateFilter', descKey: 'templateFilterDesc', build: filterTemplate },
  { id: 'fft', nameKey: 'templateFft', descKey: 'templateFftDesc', build: fftTemplate },
  { id: 'can', nameKey: 'templateCan', descKey: 'templateCanDesc', build: canTemplate },
  { id: 'serial', nameKey: 'templateSerial', descKey: 'templateSerialDesc', build: serialTemplate },
  { id: 'custom-protocol', nameKey: 'templateCustomProtocol', descKey: 'templateCustomProtocolDesc', build: customProtocolTemplate },
  { id: 'demo', nameKey: 'templateDemo', descKey: 'templateDemoDesc', build: demoTemplate },
];

export function getTemplate(id: string): QuickStartTemplate | undefined {
  return QUICK_START_TEMPLATES.find((t) => t.id === id);
}
