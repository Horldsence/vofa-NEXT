import { describe, expect, it, afterEach } from 'vitest';
import {
  collectPartialSnapshot,
  detectPresentSections,
  parseSnapshot,
  serializeSnapshot,
  ALL_BACKUP_SECTIONS,
  type AppSnapshot,
} from '../tauri/appExport';
import { useAppStore } from '../../store/appStore';

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

  it('detectPresentSections 正确识别含有的分区', () => {
    const snap: AppSnapshot = {
      version: 2,
      exportedAt: '',
      rfNodes: [],
      rfEdges: [],
      transport: { kind: 'Serial', params: { port_name: '', baud_rate: 115200, data_bits: 8, parity: 'none', stop_bits: 'one', flow_control: 'none' } },
    };
    expect(detectPresentSections(snap)).toEqual(['nodeGraph', 'transportProtocol']);
  });
});
