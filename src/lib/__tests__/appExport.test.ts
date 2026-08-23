import { describe, expect, it, afterEach, vi } from 'vitest';
import {
  applySnapshot,
  collectPartialSnapshot,
  detectPresentSections,
  parseSnapshot,
  serializeSnapshot,
  ALL_BACKUP_SECTIONS,
  type AppSnapshot,
} from '../tauri/appExport';
import { useAppStore } from '../../store/appStore';
import { tauriMock } from '../../test/setup';

describe('appExport 拆分备份', () => {
  afterEach(() => {
    // 复位图状态, 避免跨用例污染
    useAppStore.setState({ rfNodes: [], rfEdges: [] } as never);
  });

  it('collectPartialSnapshot 仅包含所选分区字段', () => {
    const snap = collectPartialSnapshot(['nodeGraph', 'windowLayout']);
    expect(snap.sections).toEqual(['nodeGraph', 'windowLayout']);
    expect(snap.rfNodes).toBeDefined();
    expect(snap.rfEdges).toBeDefined();
    expect(snap.dockRoot).toBeDefined();
    expect(snap.dockCards).toBeDefined();
    expect(snap.sidebarDock).toBeDefined();
    // 未选分区应为 undefined
    expect(snap.settings).toBeUndefined();
    expect(snap.protocol).toBeUndefined();
    expect(snap.transport).toBeUndefined();
    expect(snap.widgets).toBeUndefined();
    expect(snap.controlTabs).toBeUndefined();
    expect(snap.dataTabs).toBeUndefined();
  });

  it('serialize + parse 往返保留分区标记', () => {
    // 预置一对全局节点, transportProtocol 分区才有内容
    useAppStore.setState({
      rfNodes: [
        {
          id: 'transport-1',
          type: 'transport',
          position: { x: 0, y: 0 },
          data: {
            global: true,
            config: { kind: 'Serial', params: { port_name: '', baud_rate: 115200, data_bits: 8, parity: 'none', stop_bits: 'one', flow_control: 'none' } },
            label: 'Serial',
          },
        },
        {
          id: 'protocol-1',
          type: 'protocol',
          position: { x: 200, y: 0 },
          data: { global: true, config: { kind: 'JustFloat', channels: 4 }, convertTo: null, channels: 4, label: 'JustFloat' },
        },
      ],
      rfEdges: [
        { id: 'e-tp', source: 'transport-1', sourceHandle: 'rx', target: 'protocol-1', targetHandle: 'in' },
      ],
    } as never);
    const snap = collectPartialSnapshot(['settings', 'transportProtocol']);
    const parsed = parseSnapshot(serializeSnapshot(snap));
    expect(parsed.version).toBe(3);
    expect(detectPresentSections(parsed)).toEqual(['settings', 'transportProtocol']);
  });

  it('parseSnapshot 拒绝非法 JSON 与不支持版本', () => {
    expect(() => parseSnapshot('{oops')).toThrow();
    expect(() => parseSnapshot(JSON.stringify({ version: 99, rfNodes: [] }))).toThrow();
    expect(() => parseSnapshot(JSON.stringify({ version: 3 }))).toThrow();
  });

  it('v1 全量备份迁移为 v3 (单例传输/协议 → 全局节点)', () => {
    const v1: AppSnapshot = {
      version: 1,
      exportedAt: '2024-01-01T00:00:00Z',
      settings: {} as never,
      protocol: { kind: 'JustFloat', channels: 2 } as never,
      transport: { kind: 'Udp', params: { local_addr: '0.0.0.0', remote_addr: '127.0.0.1', local_port: 8888, remote_port: 9999 } } as never,
      widgets: [],
      controlTabs: [],
      dataTabs: [],
      activeDataTabId: 'waveform-fixed',
      activeControlTabId: 'default',
      rfNodes: [],
      rfEdges: [],
      rawDataViewPrefs: {},
    };
    const parsed = parseSnapshot(serializeSnapshot(v1 as never));
    expect(parsed.version).toBe(3);
    expect(parsed.dockRoot).toBeUndefined();
    // 单例配置迁移为一对全局节点 + rx→in 字节边
    const transport = parsed.rfNodes?.find((n) => n.type === 'transport' && n.data?.global === true);
    const protocol = parsed.rfNodes?.find((n) => n.type === 'protocol' && n.data?.global === true);
    expect((transport?.data as { config: { kind: string } }).config.kind).toBe('Udp');
    expect((protocol?.data as { config: { kind: string } }).config.kind).toBe('JustFloat');
    expect(
      parsed.rfEdges?.some(
        (e) => e.source === transport!.id && e.sourceHandle === 'rx' && e.target === protocol!.id && e.targetHandle === 'in'
      )
    ).toBe(true);
    // 顶层单例字段已移除
    expect(parsed.transport).toBeUndefined();
    expect(parsed.protocol).toBeUndefined();
    // 无 sections 字段 → 视为全量
    expect(ALL_BACKUP_SECTIONS.length).toBe(5);
  });

  it('v2 旧图迁移: 通道源 chN 出边改指 Protocol 节点, loopbackIn → in', () => {
    const v2 = {
      version: 2,
      exportedAt: '2024-01-01T00:00:00Z',
      rfNodes: [
        { id: '__channel_source__-default', type: 'channelSource', position: { x: 40, y: 40 }, data: { tabId: 'default', channelCount: 4 } },
        { id: 'w1', type: 'widget', position: { x: 300, y: 80 }, data: { tabId: 'default', widget: { kind: 'Gauge', params: { id: 'w1' } } } },
        { id: 'fd1', type: 'widget', position: { x: 300, y: 200 }, data: { tabId: 'default', widget: { kind: 'FrameDecoder', params: { id: 'fd1' } } } },
      ],
      rfEdges: [
        { id: 'e1', source: '__channel_source__-default', sourceHandle: 'ch0', target: 'w1', targetHandle: 'value' },
        { id: 'e2', source: 'cmd1', sourceHandle: 'loopbackOut', target: 'fd1', targetHandle: 'loopbackIn' },
      ],
    };
    const parsed = parseSnapshot(JSON.stringify(v2));
    expect(parsed.version).toBe(3);
    // 通道源节点删除
    expect(parsed.rfNodes?.some((n) => n.type === 'channelSource')).toBe(false);
    const protocol = parsed.rfNodes?.find((n) => n.type === 'protocol');
    // ch0 出边改指 Protocol 节点
    const chEdge = parsed.rfEdges?.find((e) => e.id === 'e1');
    expect(chEdge?.source).toBe(protocol!.id);
    expect(chEdge?.sourceHandle).toBe('ch0');
    // loopbackIn → in
    const lbEdge = parsed.rfEdges?.find((e) => e.id === 'e2');
    expect(lbEdge?.targetHandle).toBe('in');
  });

  it('v3 快照迁移幂等', () => {
    const snap: AppSnapshot = {
      version: 3,
      exportedAt: '',
      rfNodes: [
        { id: 'transport-1', type: 'transport', position: { x: 0, y: 0 }, data: { global: true, config: { kind: 'Serial', params: { port_name: '', baud_rate: 115200, data_bits: 8, parity: 'none', stop_bits: 'one', flow_control: 'none' } }, label: 'Serial' } },
      ],
      rfEdges: [],
    };
    const parsed = parseSnapshot(serializeSnapshot(snap));
    expect(parsed.version).toBe(3);
    expect(parsed.rfNodes?.length).toBe(1);
    expect(parsed.rfEdges?.length).toBe(0);
  });

  it('v3 旧版 Command 单帧配置迁移为 frames (widgets 数组 + 节点内嵌 widget 同步)', () => {
    const legacyParams = {
      id: 'cmd1',
      label: 'Cmd',
      blocks: [{ id: 'b1', type: 'const_hex', hex: 'AA' }],
      appendNewline: true,
      loopbackEnabled: false,
      sendMode: 'manual',
      timerMs: 100,
      loopbackHistory: [],
    };
    const snap = {
      version: 3,
      exportedAt: '2024-01-01T00:00:00Z',
      widgets: [{ kind: 'Command', params: legacyParams }],
      rfNodes: [
        { id: 'cmd1', type: 'widget', position: { x: 0, y: 0 }, data: { tabId: 'default', widget: { kind: 'Command', params: legacyParams } } },
      ],
      rfEdges: [],
    };
    const parsed = parseSnapshot(JSON.stringify(snap));
    // widgets 数组内的配置已包装为单帧
    const w = parsed.widgets?.[0] as { kind: 'Command'; params: { frames?: { blocks: unknown[]; appendNewline: boolean }[]; blocks?: unknown } };
    expect(w.params.frames).toHaveLength(1);
    expect(w.params.frames![0].blocks).toEqual(legacyParams.blocks);
    expect(w.params.frames![0].appendNewline).toBe(true);
    expect(w.params.blocks).toBeUndefined();
    // 节点内嵌 widget 同步归一化 (迁移会补全局 Transport/Protocol 节点, 按 id 查找)
    const nodeWidget = parsed.rfNodes?.find((n) => n.id === 'cmd1')?.data?.widget as typeof w;
    expect(nodeWidget.params.frames).toHaveLength(1);
    // 再次迁移幂等 (不重复包装)
    const again = parseSnapshot(serializeSnapshot(parsed));
    const w2 = again.widgets?.[0] as typeof w;
    expect(w2.params.frames).toHaveLength(1);
  });

  it('detectPresentSections 正确识别含有的分区', () => {const snap: AppSnapshot = {
      version: 2,
      exportedAt: '',
      rfNodes: [],
      rfEdges: [],
      transport: { kind: 'Serial', params: { port_name: '', baud_rate: 115200, data_bits: 8, parity: 'none', stop_bits: 'one', flow_control: 'none' } },
    };
    expect(detectPresentSections(snap)).toEqual(['nodeGraph', 'transportProtocol']);
  });

  it('v3 protocol 节点缺 schema → 按 config 工厂补齐 (幂等)', () => {
    const snap = {
      version: 3,
      exportedAt: '2024-01-01T00:00:00Z',
      rfNodes: [
        { id: 'transport-1', type: 'transport', position: { x: 0, y: 0 }, data: { global: true, config: { kind: 'TestData', params: { channels: 4, sample_rate: 100, signal: 'Sine' } }, label: 'TestData' } },
        { id: 'protocol-1', type: 'protocol', position: { x: 200, y: 0 }, data: { global: true, config: { kind: 'JustFloat', channels: 2 }, convertTo: null, channels: 2, label: 'JustFloat' } },
      ],
      rfEdges: [
        { id: 'e-tp', source: 'transport-1', sourceHandle: 'rx', target: 'protocol-1', targetHandle: 'in' },
      ],
    };
    const parsed = parseSnapshot(JSON.stringify(snap));
    const protocol = parsed.rfNodes?.find((n) => n.id === 'protocol-1');
    const schema = (protocol?.data as { schema?: { preset: string; decode: { type: string; portName?: string }[] } }).schema;
    // schema 已补齐: JustFloat 预设 = 2×field + tail
    expect(schema?.preset).toBe('justFloat');
    expect(schema?.decode).toHaveLength(3);
    expect(schema?.decode[0]).toMatchObject({ type: 'field', portName: 'ch0' });
    // 幂等: 再次迁移 schema 保持不变
    const again = parseSnapshot(serializeSnapshot(parsed));
    const schema2 = (again.rfNodes?.find((n) => n.id === 'protocol-1')?.data as { schema?: unknown }).schema;
    expect(schema2).toEqual(schema);
  });
});

describe('applySnapshot 后端图清理', () => {
  afterEach(() => {
    useAppStore.setState({
      controlTabs: [{ id: 'default', name: 'Tab 1', widgets: [] }],
      activeControlTabId: 'default',
      rfNodes: [],
      rfEdges: [],
    } as never);
  });

  it('替换 controlTabs 后对消失的 tab 调 remove_tab_graph (在存活 tab sync 之后)', async () => {
    tauriMock.invoke.mockClear();
    useAppStore.setState({
      controlTabs: [
        { id: 'default', name: 'Tab 1', widgets: [] },
        { id: 'tab-old', name: 'Old', widgets: [] },
      ],
      activeControlTabId: 'default',
      rfNodes: [],
      rfEdges: [],
    } as never);

    await applySnapshot({
      version: 3,
      exportedAt: '',
      sections: ['widgetsTabs'],
      controlTabs: [{ id: 'default', name: 'Tab 1', widgets: [] }],
    });

    await vi.waitFor(() => {
      expect(tauriMock.invoke).toHaveBeenCalledWith('remove_tab_graph', { tabId: 'tab-old' });
    });
    const calls = tauriMock.invoke.mock.calls as unknown as [string, unknown][];
    const syncIdx = calls.findIndex(
      ([cmd, args]) => cmd === 'update_tab_graph' && (args as { tabId: string }).tabId === 'default'
    );
    const removeIdx = calls.findIndex(([cmd]) => cmd === 'remove_tab_graph');
    // 先同步存活 tab (全局节点重新托管), 再移除消失的 tab
    expect(syncIdx).toBeGreaterThanOrEqual(0);
    expect(removeIdx).toBeGreaterThan(syncIdx);
  });

  it('controlTabs 未变化时不调 remove_tab_graph', async () => {
    tauriMock.invoke.mockClear();
    useAppStore.setState({
      controlTabs: [{ id: 'default', name: 'Tab 1', widgets: [] }],
      activeControlTabId: 'default',
      rfNodes: [],
      rfEdges: [],
    } as never);

    await applySnapshot({
      version: 3,
      exportedAt: '',
      sections: ['widgetsTabs'],
      controlTabs: [{ id: 'default', name: 'Tab 1', widgets: [] }],
    });

    await vi.waitFor(() => {
      expect(tauriMock.invoke).toHaveBeenCalledWith('update_tab_graph', expect.anything());
    });
    const calls = tauriMock.invoke.mock.calls as unknown as [string, unknown][];
    expect(calls.some(([cmd]) => cmd === 'remove_tab_graph')).toBe(false);
  });
});
