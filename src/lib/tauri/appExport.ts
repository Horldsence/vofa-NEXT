//! 全应用配置导出 / 导入 (单个 JSON 文件, 通过系统文件对话框)
//!
//! 备份范围: 设置 + 协议 + 传输 + 控件 + 节点图 + 数据标签页 + RawData 视图偏好 + 窗口组织
//! 用于备份 / 恢复 / 迁移到另一台机器。
//!
//! v2 起支持「拆分备份」: 快照可只含若干分区 (BackupSection), 导入时按分区应用。
//! 分区划分: 节点图 / 窗口组织 / 设置 / 传输与协议 / 控件与标签页。

import { save, open } from '@tauri-apps/plugin-dialog';
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
import { LazyStore } from '@tauri-apps/plugin-store';
import type { Node, Edge } from '@xyflow/react';
import { useAppStore } from '../../store/appStore';
import { useSettingsStore } from '../../store/settingsStore';
import { useDockStore, type DockNode, type DockCard } from '../../store/dockStore';
import { useLayoutStore, type SidebarDock } from '../../store/layoutStore';
import { applyAppearance } from '../../settings/applyTheme';
import type { AppSettings } from '../../settings/defaults';
import type { ControlTab, DataTab, ProtocolConfig, TransportConfig, WidgetConfig } from '../../types';
import { api } from './tauri';
import { rawDataPortId } from '../utils/nodeDef';
import { rawDataBuffer } from '../buffers/dataBuffer';
import { canFrameBuffer } from '../buffers/canBuffer';
import { logicSampleBuffer } from '../buffers/logicBuffer';
import { notify, formatError } from './notifications';
import { t } from '../../i18n';
import { getAllRawDataViewPrefs, useRawDataViewStore, type RawDataViewPrefs } from '../buffers/rawDataViewStore';

/// 备份分区 — 拆分备份/模板的最小单元
export type BackupSection =
  | 'nodeGraph'        // 节点图 (rfNodes + rfEdges)
  | 'windowLayout'     // 窗口组织 (Dock 布局树 + 侧边栏停靠)
  | 'settings'         // 应用设置
  | 'transportProtocol' // 传输 + 协议配置
  | 'widgetsTabs';     // 控件 + 标签页 + 活动页 + RawData 视图偏好

/// 全部分区 (导出全量备份时的固定顺序)
export const ALL_BACKUP_SECTIONS: BackupSection[] = [
  'nodeGraph',
  'windowLayout',
  'settings',
  'transportProtocol',
  'widgetsTabs',
];

/// 备份快照 schema — version 2
/// 各分区字段均为可选: 缺省 (sections 未提供) = 全量; 拆分备份仅含所选分区字段。
export interface AppSnapshot {
  version: 2 | 1;
  exportedAt: string;
  /// 该快照包含的分区; 缺省 = 全部 (兼容旧 v1 全量备份)
  sections?: BackupSection[];
  settings?: AppSettings;
  protocol?: ProtocolConfig;
  transport?: TransportConfig;
  widgets?: WidgetConfig[];
  controlTabs?: ControlTab[];
  dataTabs?: DataTab[];
  activeDataTabId?: string;
  activeControlTabId?: string;
  rfNodes?: Node[];
  rfEdges?: Edge[];
  rawDataViewPrefs?: Record<string, unknown>;
  /// 窗口组织 (v2 新增)
  dockRoot?: DockNode;
  dockCards?: Record<string, DockCard>;
  sidebarDock?: SidebarDock;
}

const JSON_FILTERS = [{ name: 'JSON', extensions: ['json'] }];

// ==================== 文件对话框包装 ====================

/** 通过系统"另存为"对话框将数据写入 JSON 文件。
 *  用户取消返回 false, 写入失败返回 false, 成功返回 true。 */
export async function saveJsonFile(
  filename: string,
  data: unknown
): Promise<boolean> {
  try {
    const path = await save({
      defaultPath: `${filename}.json`,
      filters: JSON_FILTERS,
    });
    // 用户取消 — save 返回 null (或 undefined)
    if (!path) return false;
    const text =
      typeof data === 'string' ? data : JSON.stringify(data, null, 2);
    await writeTextFile(path, text);
    return true;
  } catch (e) {
    console.warn('[appExport] 保存文件失败:', e);
    return false;
  }
}

/** 通过系统"打开"对话框读取并解析备份文件; 用户取消/失败返回 null。 */
export async function readSnapshotFromFile(): Promise<AppSnapshot | null> {
  try {
    const selected = await open({
      multiple: false,
      directory: false,
      filters: JSON_FILTERS,
    });
    if (!selected) return null;
    const path = Array.isArray(selected) ? selected[0] : selected;
    if (!path) return null;
    const json = await readTextFile(path);
    return parseSnapshot(json);
  } catch (e) {
    const lang = useAppStore.getState().lang;
    notify.error(t(lang, 'backupImportFailed'), formatError(e), { source: 'importConfig' });
    return null;
  }
}

// ==================== 快照收集 / 序列化 / 解析 ====================

/// 读取所有 store 的当前状态并生成全量快照。
/// rfNodes/rfEdges 经 JSON 往返确保无函数 / undefined 等不可序列化字段。
export function collectSnapshot(): AppSnapshot {
  const app = useAppStore.getState();
  return {
    version: 2,
    exportedAt: new Date().toISOString(),
    sections: ALL_BACKUP_SECTIONS,
    settings: useSettingsStore.getState().settings,
    protocol: app.protocolConfig,
    transport: app.transportConfig,
    widgets: app.widgets,
    controlTabs: app.controlTabs,
    dataTabs: app.dataTabs,
    activeDataTabId: app.activeDataTabId,
    activeControlTabId: app.activeControlTabId,
    rfNodes: JSON.parse(JSON.stringify(app.rfNodes)) as Node[],
    rfEdges: JSON.parse(JSON.stringify(app.rfEdges)) as Edge[],
    rawDataViewPrefs: getAllRawDataViewPrefs(),
    dockRoot: useDockStore.getState().root,
    dockCards: useDockStore.getState().cards,
    sidebarDock: useLayoutStore.getState().sidebarDock,
  };
}

/// 仅收集指定分区的快照 (拆分备份导出)
export function collectPartialSnapshot(sections: BackupSection[]): AppSnapshot {
  const full = collectSnapshot();
  const partial: AppSnapshot = {
    version: 2,
    exportedAt: full.exportedAt,
    sections: [...sections],
  };
  if (sections.includes('settings')) partial.settings = full.settings;
  if (sections.includes('transportProtocol')) {
    partial.protocol = full.protocol;
    partial.transport = full.transport;
  }
  if (sections.includes('widgetsTabs')) {
    partial.widgets = full.widgets;
    partial.controlTabs = full.controlTabs;
    partial.dataTabs = full.dataTabs;
    partial.activeDataTabId = full.activeDataTabId;
    partial.activeControlTabId = full.activeControlTabId;
    partial.rawDataViewPrefs = full.rawDataViewPrefs;
  }
  if (sections.includes('nodeGraph')) {
    partial.rfNodes = full.rfNodes;
    partial.rfEdges = full.rfEdges;
  }
  if (sections.includes('windowLayout')) {
    partial.dockRoot = full.dockRoot;
    partial.dockCards = full.dockCards;
    partial.sidebarDock = full.sidebarDock;
  }
  return partial;
}

export function serializeSnapshot(snap: AppSnapshot): string {
  return JSON.stringify(snap, null, 2);
}

/// 解析备份 JSON 并做最小校验, 非法时抛出带清晰信息的 Error。
/// v1 全量备份自动迁移为 v2 (无窗口组织字段)。
export function parseSnapshot(json: string): AppSnapshot {
  let parsed: unknown;
  try {
    parsed = JSON.parse(json);
  } catch {
    throw new Error('备份文件不是有效的 JSON');
  }
  const data = parsed as Partial<AppSnapshot>;
  if (!data || typeof data !== 'object') {
    throw new Error('备份文件格式无效');
  }
  if (data.version !== 1 && data.version !== 2) {
    throw new Error(`不支持的备份版本: ${String(data.version)}`);
  }
  const hasContent = (
    [
      'settings', 'protocol', 'transport', 'widgets', 'controlTabs',
      'dataTabs', 'rfNodes', 'rfEdges', 'dockRoot',
    ] as const
  ).some((k) => data[k] != null);
  if (!hasContent) {
    throw new Error('备份文件为空或缺少有效内容');
  }
  return { ...data, version: 2, exportedAt: data.exportedAt ?? new Date().toISOString() } as AppSnapshot;
}

/// 检测快照实际包含哪些分区 (供拆分备份导入时的勾选预填), 按 ALL_BACKUP_SECTIONS 顺序返回
export function detectPresentSections(snap: AppSnapshot): BackupSection[] {
  const has = (s: BackupSection): boolean => {
    switch (s) {
      case 'settings': return snap.settings != null;
      case 'transportProtocol': return snap.protocol != null || snap.transport != null;
      case 'widgetsTabs': return snap.widgets != null || snap.controlTabs != null || snap.dataTabs != null;
      case 'nodeGraph': return snap.rfNodes != null || snap.rfEdges != null;
      case 'windowLayout': return snap.dockRoot != null || snap.dockCards != null;
    }
  };
  return ALL_BACKUP_SECTIONS.filter(has);
}

// ==================== 状态恢复 ====================

/// 将 data 分类的缓存容量设置同步到后端与前端 buffer 实例
/// (与 settingsStore.applyDataCapacity 相同 — 该函数未导出, 此处复刻)
function applyDataCapacity(settings: AppSettings) {
  const data = settings.data;
  api.setWaveformBufferCapacity(data.waveformBufferPoints).catch((e: unknown) =>
    console.warn('[appExport] 设置波形缓冲区容量失败:', e)
  );
  api.setRawDataBufferCapacity(data.rawDataBufferBytes).catch((e: unknown) =>
    console.warn('[appExport] 设置原始数据缓冲区容量失败:', e)
  );
  api.setCanBufferCapacity(data.canBufferFrames).catch((e: unknown) =>
    console.warn('[appExport] 设置 CAN 缓冲区容量失败:', e)
  );
  api.setLogicBufferCapacity(data.logicBufferSamples).catch((e: unknown) =>
    console.warn('[appExport] 设置逻辑缓冲区容量失败:', e)
  );
  rawDataBuffer.setCapacity(data.rawDataBufferBytes);
  canFrameBuffer.setCapacity(data.canBufferFrames);
  logicSampleBuffer.setCapacity(data.logicBufferSamples);
}

/// 恢复设置到 settings store, 并同步应用到磁盘 / 主题 / 容量
async function applySettings(settings: AppSettings): Promise<void> {
  useSettingsStore.setState({ settings });
  applyAppearance(settings.appearance);
  applyDataCapacity(settings);
  // 持久化到磁盘 (settings.json), 保证重启后仍生效
  try {
    const store = new LazyStore('settings.json');
    await store.set('app', settings);
  } catch (e) {
    console.warn('[appExport] 设置持久化失败:', e);
  }
}

/// 解析要应用的分区集合 (显式指定 → 快照声明 → 全部)
function resolveSections(
  snap: AppSnapshot,
  opts?: { sections?: BackupSection[] }
): BackupSection[] {
  if (opts?.sections?.length) return opts.sections;
  if (snap.sections?.length) return snap.sections;
  return ALL_BACKUP_SECTIONS;
}

/// 将快照按分区应用到所有 store (恢复 / 模板应用)
export async function applySnapshot(
  snap: AppSnapshot,
  opts?: { sections?: BackupSection[] }
): Promise<void> {
  const want = new Set(resolveSections(snap, opts));
  const app = useAppStore.getState();

  // 1. 设置
  if (want.has('settings') && snap.settings) {
    await applySettings(snap.settings);
  }

  // 2. 传输 + 协议 (全局单例)
  if (want.has('transportProtocol')) {
    if (snap.protocol) await app.setProtocolConfig(snap.protocol);
    if (snap.transport) app.setTransportConfig(snap.transport);
  }

  // 3. 控件 + 标签页 + 活动页 + RawData 视图偏好
  if (want.has('widgetsTabs')) {
    const patch: Record<string, unknown> = {};
    if (snap.widgets) patch.widgets = snap.widgets;
    if (snap.controlTabs) patch.controlTabs = snap.controlTabs;
    if (snap.dataTabs) patch.dataTabs = snap.dataTabs;
    if (snap.activeDataTabId != null) patch.activeDataTabId = snap.activeDataTabId;
    if (snap.activeControlTabId != null) patch.activeControlTabId = snap.activeControlTabId;
    useAppStore.setState(patch);
    if (snap.rawDataViewPrefs) {
      useRawDataViewStore.setState({
        prefsByWidget: snap.rawDataViewPrefs as Record<string, RawDataViewPrefs>,
      });
    }
  }

  // 4. 节点图
  if (want.has('nodeGraph') && snap.rfNodes && snap.rfEdges) {
    // 迁移旧版快照: 连到 RawData 的边可能还带着回退端口 'data' 作为 targetHandle,
    // 而 RawData 的端口是动态派生的 (`src:<source>:<handle>`) — 不归一化会导致
    // React Flow 找不到 handle (warning #008), 边无法渲染
    const rawDataNodeIds = new Set(
      snap.rfNodes
        .filter((n) => (n.data?.widget as WidgetConfig | undefined)?.kind === 'RawData')
        .map((n) => n.id)
    );
    const rfEdges = snap.rfEdges.map((e) =>
      rawDataNodeIds.has(e.target) && !e.targetHandle?.startsWith('src:')
        ? { ...e, targetHandle: rawDataPortId(e.source, e.sourceHandle) }
        : e
    );
    useAppStore.setState({ rfNodes: snap.rfNodes, rfEdges });
  }

  // 5. 窗口组织
  if (want.has('windowLayout') && snap.dockRoot && snap.dockCards) {
    useDockStore.setState({
      root: snap.dockRoot,
      cards: snap.dockCards,
      focusedCardId: null,
    });
    if (snap.sidebarDock) useLayoutStore.setState({ sidebarDock: snap.sidebarDock });
  }

  // 6. 重新同步后端节点图 (节点图或标签页变化后)
  if (want.has('nodeGraph') || want.has('widgetsTabs')) {
    for (const tab of useAppStore.getState().controlTabs) {
      useAppStore.getState().syncTabGraph(tab.id);
    }
  }
}

// ==================== 导出 / 导入入口 ====================

/// 导出完整配置到文件 (用户选择保存位置)
export async function exportAppToFile(): Promise<void> {
  const lang = useAppStore.getState().lang;
  try {
    const snap = collectSnapshot();
    const json = serializeSnapshot(snap);
    const ok = await saveJsonFile('vofa-next-backup', json);
    if (ok) {
      notify.info(t(lang, 'backupExportSuccess'), t(lang, 'backupExportSuccessDesc'), {
        source: 'exportConfig',
      });
    }
    // 用户取消时不提示
  } catch (e) {
    notify.error(t(lang, 'backupExportFailed'), formatError(e), { source: 'exportConfig' });
  }
}

/// 从文件导入完整配置 (用户选择文件)
export async function importAppFromFile(): Promise<void> {
  const lang = useAppStore.getState().lang;
  try {
    const snap = await readSnapshotFromFile();
    if (!snap) return;
    await applySnapshot(snap);

    // 若当前已连接, 提示用户重新连接以应用导入的传输配置 (不自动连接)
    const isConnected = useAppStore.getState().connectionState === 'Connected';
    notify.info(
      t(lang, 'backupImportSuccess'),
      isConnected ? t(lang, 'backupImportSuccessDescReconnect') : t(lang, 'backupImportSuccessDesc'),
      { source: 'importConfig' }
    );
  } catch (e) {
    notify.error(t(lang, 'backupImportFailed'), formatError(e), { source: 'importConfig' });
  }
}

/// 拆分备份: 仅导出所选分区到文件。返回是否成功 (false = 取消/失败)。
export async function exportSectionsToFile(sections: BackupSection[]): Promise<boolean> {
  const lang = useAppStore.getState().lang;
  try {
    const snap = collectPartialSnapshot(sections);
    const ok = await saveJsonFile('vofa-next-backup', serializeSnapshot(snap));
    if (ok) {
      notify.info(t(lang, 'backupExportSuccess'), t(lang, 'backupExportSuccessDesc'), {
        source: 'exportConfig',
      });
    }
    return ok;
  } catch (e) {
    notify.error(t(lang, 'backupExportFailed'), formatError(e), { source: 'exportConfig' });
    return false;
  }
}

/// 拆分备份: 从文件导入指定分区 (缺省 = 文件声明的分区 / 全部)。
export async function importSectionsFromFile(
  sections?: BackupSection[]
): Promise<boolean> {
  const lang = useAppStore.getState().lang;
  try {
    const snap = await readSnapshotFromFile();
    if (!snap) return false;
    await applySnapshot(snap, sections ? { sections } : undefined);
    const isConnected = useAppStore.getState().connectionState === 'Connected';
    notify.info(
      t(lang, 'backupImportSuccess'),
      isConnected ? t(lang, 'backupImportSuccessDescReconnect') : t(lang, 'backupImportSuccessDesc'),
      { source: 'importConfig' }
    );
    return true;
  } catch (e) {
    notify.error(t(lang, 'backupImportFailed'), formatError(e), { source: 'importConfig' });
    return false;
  }
}
