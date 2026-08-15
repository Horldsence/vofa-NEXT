//! 快速开始模板 — 内置节点图 + 窗口组织 + 传输/协议配置
//!
//! 每个模板返回一份「类快照」(AppSnapshot), 应用时:
//!   - 替换: 清空工作区后按分区应用 (不含设置)
//!   - 合并: 作为新控件标签页追加 (见 applyTemplate.ts 的 ID 重映射)
//!
//! 内置模板: 数学 / 滤波器 / CAN / 串口 / 综合演示 (TestData, 无硬件可跑)。

import { type Node, type Edge } from '@xyflow/react';
import { createWidget } from '../utils/createWidget';
import { CHANNEL_SOURCE_ID } from '../../store/appStore';
import { ALL_BACKUP_SECTIONS, type AppSnapshot } from '../tauri/appExport';
import type { DockNode, DockCard } from '../../store/dockStore';
import type { WidgetConfig, DataTab, ControlTab, TransportConfig, ProtocolConfig } from '../../types';

const TAB = 'default';
const SOURCE_ID = `${CHANNEL_SOURCE_ID}-${TAB}`;

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
  return { id, type: 'widget', position: { x, y }, data: { widget: w, tabId: TAB } };
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

/// 通道源节点 (内联实现, 避免引入 appStoreHelpers 造成循环依赖)
function source(channels = 4): Node {
  return {
    id: `${CHANNEL_SOURCE_ID}-${TAB}`,
    type: 'channelSource',
    position: { x: 40, y: 40 },
    data: { tabId: TAB, channelCount: channels, label: 'Channel Source' },
    selectable: false,
    deletable: false,
  };
}

/// 固定波形数据 Tab (始终存在, 不可关闭)
const WAVEFORM_FIXED: DataTab = {
  id: 'waveform-fixed',
  type: 'waveform',
  name: 'Waveform',
  closable: false,
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

/// 默认 Dock 布局: 上控件卡 / 下数据卡
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
    'data-main': {
      id: 'data-main',
      kind: 'data',
      tabIds: dataTabIds,
      activeTabId: activeDataTabId ?? dataTabIds[0] ?? null,
    },
  };
  const root: DockNode = {
    id: 'split-root',
    type: 'split',
    dir: 'col',
    children: [
      { id: 'node-control', type: 'card', cardId: 'control-main' },
      { id: 'node-data', type: 'card', cardId: 'data-main' },
    ],
    sizes: [45, 55],
  };
  return { dockRoot: root, dockCards: cards };
}

/// 组装快照
function buildSnapshot(opts: {
  name: string;
  protocol: ProtocolConfig;
  transport: TransportConfig;
  widgetNodes: Node[];
  edges: Edge[];
  extraDataTabs?: DataTab[];
  activeDataTabId?: string;
  sourceChannels?: number;
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
  const dataTabs: DataTab[] = [WAVEFORM_FIXED, ...derivedTabs, ...extraTabs];
  const activeDataTabId =
    opts.activeDataTabId ?? derivedTabs[0]?.id ?? extraTabs[0]?.id ?? 'waveform-fixed';
  const { dockRoot, dockCards } = buildDock(
    [TAB],
    dataTabs.map((t) => t.id),
    activeDataTabId
  );

  return {
    version: 2,
    exportedAt: new Date().toISOString(),
    sections: ALL_BACKUP_SECTIONS,
    // 模板不含设置 — 应用时保留用户当前设置
    protocol: opts.protocol,
    transport: opts.transport,
    widgets,
    controlTabs,
    dataTabs,
    activeDataTabId,
    activeControlTabId: TAB,
    rfNodes: [source(opts.sourceChannels ?? 4), ...opts.widgetNodes],
    rfEdges: opts.edges,
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
      edge('e1', SOURCE_ID, 'ch0', 'm-sin', 'in0'),
      edge('e2', 'm-sin', 'result', 'nd-sin', 'value'),
      edge('e3', SOURCE_ID, 'ch0', 'm-add', 'in0'),
      edge('e4', SOURCE_ID, 'ch1', 'm-add', 'in1'),
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
      edge('e1', SOURCE_ID, 'ch0', 'f-lp', 'in0'),
      edge('e2', 'f-lp', 'result', 'nd-lp', 'value'),
      edge('e3', SOURCE_ID, 'ch0', 'wf-filter', 'CH0'),
      edge('e4', 'f-lp', 'result', 'wf-filter', 'CH1'),
    ],
    activeDataTabId: 'wf-filter',
  });
}

function canTemplate(): AppSnapshot {
  return buildSnapshot({
    name: 'CAN',
    protocol: { kind: 'Slcan' },
    transport: {
      kind: 'Slcan',
      params: { port_name: '', baud_rate: 115200, can_bitrate: 'bps500k' },
    },
    widgetNodes: [],
    edges: [],
    extraDataTabs: [{ id: 'can-main', type: 'can', name: 'CAN', closable: true }],
    activeDataTabId: 'can-main',
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
      edge('e1', SOURCE_ID, 'ch0', 'g-ch0', 'value'),
      edge('e2', SOURCE_ID, 'ch1', 'g-ch1', 'value'),
      edge('e3', SOURCE_ID, 'ch2', 'nd-ch2', 'value'),
      edge('e4', SOURCE_ID, 'ch3', 'nd-ch3', 'value'),
    ],
    activeDataTabId: 'waveform-fixed',
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
      edge('e1', SOURCE_ID, 'ch0', 'f-lp', 'in0'),
      edge('e2', 'f-lp', 'result', 'nd-lp', 'value'),
      edge('e3', 'f-lp', 'result', 'wf', 'CH0'),
      edge('e4', SOURCE_ID, 'ch1', 'm-sin', 'in0'),
      edge('e5', 'm-sin', 'result', 'g-sin', 'value'),
    ],
    activeDataTabId: 'wf',
  });
}

export const QUICK_START_TEMPLATES: QuickStartTemplate[] = [
  { id: 'math', nameKey: 'templateMath', descKey: 'templateMathDesc', build: mathTemplate },
  { id: 'filter', nameKey: 'templateFilter', descKey: 'templateFilterDesc', build: filterTemplate },
  { id: 'can', nameKey: 'templateCan', descKey: 'templateCanDesc', build: canTemplate },
  { id: 'serial', nameKey: 'templateSerial', descKey: 'templateSerialDesc', build: serialTemplate },
  { id: 'demo', nameKey: 'templateDemo', descKey: 'templateDemoDesc', build: demoTemplate },
];

export function getTemplate(id: string): QuickStartTemplate | undefined {
  return QUICK_START_TEMPLATES.find((t) => t.id === id);
}
