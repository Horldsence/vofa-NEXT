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
import { useAppStore } from '../../store/appStore';
import type { WidgetConfig } from '../../types';

describe('快速开始模板', () => {
  it('包含数学 / 滤波器 / 频谱分析 / CAN / 串口 / 演示', () => {
    const ids = QUICK_START_TEMPLATES.map((t) => t.id);
    expect(ids).toEqual(expect.arrayContaining(['math', 'filter', 'fft', 'can', 'serial', 'demo']));
  });

  it('每个模板生成的快照结构完整且自洽 (v3: 全局 Transport + Protocol 节点)', () => {
    for (const tpl of QUICK_START_TEMPLATES) {
      const snap = tpl.build();
      expect(snap.version).toBe(3);
      // 全局 Transport / Protocol 节点存在
      const transport = snap.rfNodes?.find((n) => n.type === 'transport' && n.data?.global === true);
      const protocol = snap.rfNodes?.find((n) => n.type === 'protocol' && n.data?.global === true);
      expect(transport).toBeDefined();
      expect(protocol).toBeDefined();
      // 字节边 Transport.rx → Protocol.in 存在
      expect(
        snap.rfEdges?.some(
          (e) => e.source === transport!.id && e.sourceHandle === 'rx' && e.target === protocol!.id && e.targetHandle === 'in'
        )
      ).toBe(true);
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

  it('CAN 模板使用 Slcan 传输与协议 (全局节点)', () => {
    const snap = getTemplate('can')!.build();
    const transport = snap.rfNodes?.find((n) => n.type === 'transport');
    const protocol = snap.rfNodes?.find((n) => n.type === 'protocol');
    expect((transport?.data as { config: unknown }).config).toEqual({
      kind: 'Slcan',
      params: { port_name: '', baud_rate: 115200, can_bitrate: 'bps500k' },
    });
    expect((protocol?.data as { config: unknown }).config).toEqual({ kind: 'Slcan' });
    expect(snap.dataTabs?.some((t) => t.type === 'can')).toBe(true);
  });

  it('串口模板使用 Serial 传输', () => {
    const snap = getTemplate('serial')!.build();
    const transport = snap.rfNodes?.find((n) => n.type === 'transport');
    const protocol = snap.rfNodes?.find((n) => n.type === 'protocol');
    expect((transport?.data as { config: { kind: string } }).config.kind).toBe('Serial');
    expect((protocol?.data as { config: { kind: string } }).config.kind).toBe('JustFloat');
  });

  it('每个模板的 Protocol 节点都携带帧 schema (预设由工厂生成)', () => {
    for (const tpl of QUICK_START_TEMPLATES) {
      const snap = tpl.build();
      const protocol = snap.rfNodes?.find((n) => n.type === 'protocol');
      const schema = (protocol?.data as { schema?: { preset: string; decode: unknown[] } }).schema;
      expect(schema, `模板 ${tpl.id} 缺 schema`).toBeDefined();
      expect(Array.isArray(schema!.decode)).toBe(true);
    }
  });

  it('自定义协议模板: custom schema 命名端口 + 多帧 CommandSender', () => {
    const snap = getTemplate('custom-protocol')!.build();
    const protocol = snap.rfNodes?.find((n) => n.type === 'protocol');
    const data = protocol?.data as {
      schema: { preset: string; legacyConfig?: unknown; decode: { type: string; portName?: string }[] };
    };
    expect(data.schema.preset).toBe('custom');
    expect(data.schema.legacyConfig ?? null).toBeNull();
    // 命名端口 speed/temp 由 field 块派生
    const ports = data.schema.decode.filter((b) => b.type === 'field').map((b) => b.portName);
    expect(ports).toEqual(['speed', 'temp']);
    // 命名端口直接出现在边上 (后端 ProtocolSource 槽位支持命名端口)
    expect(snap.rfEdges?.some((e) => e.sourceHandle === 'speed')).toBe(true);
    expect(snap.rfEdges?.some((e) => e.sourceHandle === 'temp')).toBe(true);
    // 多帧 CommandSender
    const cmd = snap.widgets?.find((w) => w.kind === 'Command');
    const frames = (cmd?.params as { frames?: unknown[] }).frames;
    expect(Array.isArray(frames)).toBe(true);
    expect(frames!.length).toBeGreaterThanOrEqual(2);
  });

  it('模板快照过迁移后幂等 (schema 已带, 不再补齐)', async () => {
    const { migrateSnapshotToV3 } = await import('../tauri/appExport');
    for (const tpl of QUICK_START_TEMPLATES) {
      const snap = tpl.build();
      const once = migrateSnapshotToV3(snap);
      const twice = migrateSnapshotToV3(once);
      expect(JSON.stringify(twice)).toBe(JSON.stringify(once));
    }
  });
});

describe('applyTemplate 合并模式', () => {
  beforeEach(() => {
    // 复位到默认基线 (无全局节点)
    useAppStore.setState({
      widgets: [],
      controlTabs: [{ id: 'default', name: 'Tab 1', widgets: [] }],
      dataTabs: [{ id: 'waveform-fixed', type: 'waveform', name: 'Waveform', closable: false }],
      activeControlTabId: 'default',
      activeDataTabId: 'waveform-fixed',
      rfNodes: [],
      rfEdges: [],
    } as never);
  });

  it('合并后追加新控件标签页并重映射 ID, 全局节点不重复导入', async () => {
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
    // 新控件节点均归入新标签页; 全局节点导入一份 (基线无主节点)
    const newNodes = after.rfNodes.filter((n) => n.data?.tabId === newTab!.id);
    const templateWidgetNodeCount = (snap.rfNodes ?? []).filter((n) => n.type === 'widget').length;
    expect(newNodes.length).toBe(templateWidgetNodeCount);
    expect(after.rfNodes.filter((n) => n.data?.global === true).length).toBe(2);
  });

  it('替换模式覆盖现有节点图与控件 (含全局节点)', async () => {
    const snap = getTemplate('filter')!.build();
    await applyTemplate(snap, 'replace');

    const after = useAppStore.getState();
    expect(after.controlTabs.length).toBe(1);
    const widgets = after.widgets;
    expect(widgets.length).toBe((snap.widgets ?? []).length);
    // 替换后 rfNodes 与快照一致 (含全局 Transport/Protocol 节点)
    expect(after.rfNodes.length).toBe((snap.rfNodes ?? []).length);
    expect(after.rfNodes.some((n) => n.type === 'transport')).toBe(true);
    expect(after.rfNodes.some((n) => n.type === 'protocol')).toBe(true);
  });
});
