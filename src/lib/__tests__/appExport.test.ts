import { describe, expect, it } from 'vitest';
import {
  collectPartialSnapshot,
  detectPresentSections,
  parseSnapshot,
  serializeSnapshot,
  ALL_BACKUP_SECTIONS,
  type AppSnapshot,
} from '../tauri/appExport';

describe('appExport 拆分备份', () => {
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
    const snap = collectPartialSnapshot(['settings', 'transportProtocol']);
    const parsed = parseSnapshot(serializeSnapshot(snap));
    expect(parsed.version).toBe(2);
    expect(detectPresentSections(parsed)).toEqual(['settings', 'transportProtocol']);
  });

  it('parseSnapshot 拒绝非法 JSON 与不支持版本', () => {
    expect(() => parseSnapshot('{oops')).toThrow();
    expect(() => parseSnapshot(JSON.stringify({ version: 99, rfNodes: [] }))).toThrow();
    expect(() => parseSnapshot(JSON.stringify({ version: 2 }))).toThrow();
  });

  it('v1 全量备份迁移为 v2', () => {
    const v1: AppSnapshot = {
      version: 1,
      exportedAt: '2024-01-01T00:00:00Z',
      settings: {} as never,
      protocol: {} as never,
      transport: {} as never,
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
    expect(parsed.version).toBe(2);
    expect(parsed.dockRoot).toBeUndefined();
    // 无 sections 字段 → 视为全量
    expect(ALL_BACKUP_SECTIONS.length).toBe(5);
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
