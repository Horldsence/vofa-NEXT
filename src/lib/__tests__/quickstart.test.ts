import { beforeEach, describe, expect, it, vi } from 'vitest';

// dockStore / layoutStore / rawDataViewStore 的 persist 中间件需要 localStorage — 在导入前提供内存桩
vi.hoisted(() => {
  const store = new Map<string, string>();
  const localStorageMock = {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => void store.set(key, value),
    removeItem: (key: string) => void store.delete(key),
    clear: () => store.clear(),
    key: (index: number) => [...store.keys()][index] ?? null,
    get length() {
      return store.size;
    },
  };
  const g = globalThis as { localStorage?: unknown };
  g.localStorage = localStorageMock;
});

import { QUICK_START_TEMPLATES, getTemplate } from '../quickstart/templates';
import { applyTemplate } from '../quickstart/applyTemplate';
import { useAppStore, CHANNEL_SOURCE_ID } from '../../store/appStore';
import { createChannelSourceNode } from '../../store/appStoreHelpers';
import type { WidgetConfig } from '../../types';

describe('快速开始模板', () => {
  it('包含数学 / 滤波器 / 频谱分析 / CAN / 串口 / 演示', () => {
    const ids = QUICK_START_TEMPLATES.map((t) => t.id);
    expect(ids).toEqual(expect.arrayContaining(['math', 'filter', 'fft', 'can', 'serial', 'demo']));
  });

  it('每个模板生成的快照结构完整且自洽', () => {
    for (const tpl of QUICK_START_TEMPLATES) {
      const snap = tpl.build();
      expect(snap.version).toBe(2);
      // 通道源节点存在且属于 default 标签页
      expect(snap.rfNodes?.some((n) => n.type === 'channelSource')).toBe(true);
      // 所有控件节点 tabId 均为 default
      const nodeIds = new Set(snap.rfNodes?.map((n) => n.id) ?? []);
      for (const n of snap.rfNodes ?? []) {
        if (n.type === 'widget') expect(n.data.tabId).toBe('default');
      }
      // 边两端都在节点集合中
      for (const e of snap.rfEdges ?? []) {
        expect(nodeIds.has(e.source)).toBe(true);
        expect(nodeIds.has(e.target)).toBe(true);
      }
      // 标签页与 dock 组织
      expect(snap.controlTabs?.[0]?.id).toBe('default');
      expect(snap.dataTabs?.some((t) => t.id === 'waveform-fixed')).toBe(true);
      expect(snap.dockCards?.['control-main']?.tabIds).toContain('default');
      expect(snap.dockCards?.['data-main']?.tabIds).toEqual(
        expect.arrayContaining(snap.dataTabs?.map((t) => t.id) ?? [])
      );
    }
  });

  it('CAN 模板使用 Slcan 传输与协议', () => {
    const snap = getTemplate('can')!.build();
    expect(snap.transport).toEqual({
      kind: 'Slcan',
      params: { port_name: '', baud_rate: 115200, can_bitrate: 'bps500k' },
    });
    expect(snap.protocol).toEqual({ kind: 'Slcan' });
    expect(snap.dataTabs?.some((t) => t.type === 'can')).toBe(true);
  });

  it('串口模板使用 Serial 传输', () => {
    const snap = getTemplate('serial')!.build();
    expect(snap.transport?.kind).toBe('Serial');
    expect(snap.protocol?.kind).toBe('JustFloat');
  });
});

describe('applyTemplate 合并模式', () => {
  beforeEach(() => {
    // 复位到默认基线
    useAppStore.setState({
      widgets: [],
      controlTabs: [{ id: 'default', name: 'Tab 1', widgets: [] }],
      dataTabs: [{ id: 'waveform-fixed', type: 'waveform', name: 'Waveform', closable: false }],
      activeControlTabId: 'default',
      activeDataTabId: 'waveform-fixed',
      rfNodes: [createChannelSourceNode('default', 4)],
      rfEdges: [],
    } as never);
  });

  it('合并后追加新控件标签页并重映射 ID', async () => {
    const snap = getTemplate('math')!.build();
    const before = useAppStore.getState();
    await applyTemplate(snap, 'merge');

    const after = useAppStore.getState();
    expect(after.controlTabs.length).toBe(before.controlTabs.length + 1);
    const newTab = after.controlTabs.find((t) => t.id.startsWith('tpl-'));
    expect(newTab).toBeDefined();
    expect(after.activeControlTabId).toBe(newTab!.id);
    // 模板 widget id 不应与现有 id 冲突
    const templateIds = new Set((snap.widgets ?? []).map((w) => w.params.id));
    for (const id of templateIds) {
      expect(after.widgets.some((w) => w.params.id === id)).toBe(false);
    }
    // 新节点均归入新标签页
    const newNodes = after.rfNodes.filter((n) => n.data?.tabId === newTab!.id);
    expect(newNodes.length).toBe((snap.rfNodes ?? []).length);
    // 通道源节点归属新标签页
    const sourceNode = after.rfNodes.find((n) => n.id === `${CHANNEL_SOURCE_ID}-${newTab!.id}`);
    expect(sourceNode).toBeDefined();
  });

  it('替换模式覆盖现有节点图与控件', async () => {
    const snap = getTemplate('filter')!.build();
    await applyTemplate(snap, 'replace');

    const after = useAppStore.getState();
    expect(after.controlTabs.length).toBe(1);
    const widgets = after.widgets as WidgetConfig[];
    expect(widgets.length).toBe((snap.widgets ?? []).length);
    // 替换后 rfNodes 与快照一致 (含通道源)
    expect(after.rfNodes.length).toBe((snap.rfNodes ?? []).length);
  });
});
