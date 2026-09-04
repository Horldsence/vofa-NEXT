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
import { rebaseHistory } from '../../store/historyStore';
import { useSettingsStore } from '../../store/settingsStore';
import { useDockStore, type DockNode, type DockCard } from '../../store/dockStore';
import { useLayoutStore, type SidebarDock } from '../../store/layoutStore';
import { applyAppearance } from '../../settings/applyTheme';
import type { AppSettings } from '../../settings/defaults';
import type { ControlTab, DataTab, ProtocolConfig, TransportConfig, WidgetConfig } from '../../types';
import { api } from './tauri';
import { rawDataPortId } from '../utils/nodeDef';
import { setRawDataPreviewCapacity } from '../buffers/rawDataPreviewRegistry';
import { canFrameBuffer } from '../buffers/canBuffer';
import { logicSampleBuffer } from '../buffers/logicBuffer';
import { notify, formatError } from './notifications';
import { t } from '../../i18n';
import { DEFAULT_SERIAL } from '../../store/slices/connection';
import { DEFAULT_PROTOCOL } from '../../store/slices/protocol';
import { getEffectiveChannels } from '../../store/appStoreHelpers';
import { schemaFromProtocolConfig } from '../utils/protocolSchema';
import { normalizeCommandConfig } from '../utils/commandFrames';
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

/// 备份快照 schema — version 3
/// 各分区字段均为可选: 缺省 (sections 未提供) = 全量; 拆分备份仅含所选分区字段。
/// v3: 传输/协议配置从顶层单例字段迁移为全局 Transport/Protocol 节点 (rfNodes 内),
///     顶层 transport/protocol 字段仅用于读取旧版 (v1/v2) 备份。
export interface AppSnapshot {
  version: 3 | 2 | 1;
  exportedAt: string;
  /// 该快照包含的分区; 缺省 = 全部 (兼容旧 v1 全量备份)
  sections?: BackupSection[];
  settings?: AppSettings;
  /// @deprecated v3 起传输/协议配置在全局节点 (rfNodes) 内; 仅旧版备份使用
  protocol?: ProtocolConfig;
  /// @deprecated 同上
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
    const selected: unknown = await open({
      multiple: false,
      directory: false,
      filters: JSON_FILTERS,
    });
    if (!selected) return null;
    const path: unknown = Array.isArray(selected) ? (selected as unknown[])[0] : selected;
    if (typeof path !== 'string' || path === '') return null;
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
/// v3: 传输/协议配置已包含在全局节点 (rfNodes) 内, 不再单列顶层字段。
export function collectSnapshot(): AppSnapshot {
  const app = useAppStore.getState();
  return {
    version: 3,
    exportedAt: new Date().toISOString(),
    sections: ALL_BACKUP_SECTIONS,
    // 导出剥离 API key — 密钥不入备份文件 (明文扩散面)
    settings: stripApiKey(useSettingsStore.getState().settings),
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
/// v3: 'transportProtocol' 分区 = 全局 Transport/Protocol 节点 (rfNodes 子集) + 其间的字节边
export function collectPartialSnapshot(sections: BackupSection[]): AppSnapshot {
  const full = collectSnapshot();
  const partial: AppSnapshot = {
    version: 3,
    exportedAt: full.exportedAt,
    sections: [...sections],
  };
  if (sections.includes('settings')) partial.settings = full.settings;
  if (sections.includes('transportProtocol')) {
    const globalNodes = (full.rfNodes ?? []).filter((n) => n.data?.global === true);
    const globalIds = new Set(globalNodes.map((n) => n.id));
    partial.rfNodes = globalNodes;
    partial.rfEdges = (full.rfEdges ?? []).filter(
      (e) => globalIds.has(e.source) && globalIds.has(e.target)
    );
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
    // transportProtocol 与 nodeGraph 同选时避免覆盖: nodeGraph 含全量节点
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
/// v1/v2 备份自动迁移为 v3 (无窗口组织字段 → v2; 单例传输/协议 → 全局节点)。
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
  if (data.version !== 1 && data.version !== 2 && data.version !== 3) {
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
  return migrateSnapshotToV3({
    ...data,
    exportedAt: data.exportedAt ?? new Date().toISOString(),
  } as AppSnapshot);
}

// ==================== v1/v2 → v3 迁移 ====================

/// 迁移后的固定全局节点 id (幂等: 多次迁移结果一致)
export const MIGRATED_TRANSPORT_ID = 'global-transport';
export const MIGRATED_PROTOCOL_ID = 'global-protocol';

/// 旧版通道源节点 id 前缀 (v2 及以前每 tab 一个 `__channel_source__-<tabId>`)
const LEGACY_CHANNEL_SOURCE_PREFIX = '__channel_source__';

/// 将 v1/v2 快照迁移为 v3:
/// - 顶层 transport/protocol 单例配置 → 一对全局 Transport + Protocol 节点
/// - 旧通道源节点删除, 其 chN 出边改写为从 Protocol 节点发出
/// - FrameDecoder 旧字节输入口 loopbackIn → in
/// - Command 旧版单帧配置 (blocks 在顶层) → frames 多帧结构
/// - Protocol 节点缺 schema → 按 config 工厂补齐 (协议 schema 化)
/// - 追加 Transport.rx → Protocol.in 字节边 (两者皆存在且尚无字节边时)
/// 已是 v3 的快照原样返回 (幂等)
export function migrateSnapshotToV3(snap: AppSnapshot): AppSnapshot {
  const hasLegacyChannelSource = (snap.rfNodes ?? []).some((n) => n.type === 'channelSource');
  const hasLegacySingletons = snap.transport != null || snap.protocol != null;
  const hasLegacyLoopbackIn = (snap.rfEdges ?? []).some((e) => e.targetHandle === 'loopbackIn');
  // protocol 节点缺 schema (schema 化前的 v3 快照)
  const hasProtocolMissingSchema = (snap.rfNodes ?? []).some(
    (n) => n.data?.global === true && n.type === 'protocol' && n.data.schema == null
  );
  const isLegacyCommand = (w: WidgetConfig | undefined): boolean =>
    w?.kind === 'Command' && !Array.isArray((w.params as { frames?: unknown }).frames);
  const hasLegacyCommand =
    (snap.widgets ?? []).some(isLegacyCommand) ||
    (snap.rfNodes ?? []).some((n) => isLegacyCommand(n.data?.widget as WidgetConfig | undefined));
  if (snap.version === 3 && !hasLegacyChannelSource && !hasLegacySingletons && !hasLegacyLoopbackIn && !hasLegacyCommand && !hasProtocolMissingSchema) {
    return { ...snap, version: 3 };
  }

  const rfNodes: Node[] = [];
  let transportId: string | null = null;
  let protocolId: string | null = null;

  // 已存在的全局节点优先保留 (幂等)
  for (const n of snap.rfNodes ?? []) {
    if (n.type === 'channelSource') continue; // 删除通道源节点
    if (n.data?.global === true && n.type === 'transport' && !transportId) transportId = n.id;
    if (n.data?.global === true && n.type === 'protocol') {
      protocolId ??= n.id;
      // 缺 schema → 按 config 工厂补齐 (幂等: 已有 schema 原样保留)
      if (n.data.schema == null) {
        const config = n.data.config as ProtocolConfig;
        rfNodes.push({ ...n, data: { ...n.data, schema: schemaFromProtocolConfig(config) } });
        continue;
      }
    }
    // Command 旧版单帧配置 → frames (节点内嵌 widget 与 widgets 数组需同步归一化)
    const w = n.data?.widget as WidgetConfig | undefined;
    if (isLegacyCommand(w) && w?.kind === 'Command') {
      rfNodes.push({ ...n, data: { ...n.data, widget: { kind: 'Command', params: normalizeCommandConfig(w.params) } } });
      continue;
    }
    rfNodes.push(n);
  }

  // 单例配置 → 全局节点
  if (!transportId) {
    transportId = MIGRATED_TRANSPORT_ID;
    rfNodes.unshift({
      id: transportId,
      type: 'transport',
      position: { x: 40, y: 40 },
      data: { global: true, config: snap.transport ?? DEFAULT_SERIAL, label: 'Transport' },
      selected: false,
    });
  }
  if (!protocolId) {
    protocolId = MIGRATED_PROTOCOL_ID;
    const config = snap.protocol ?? DEFAULT_PROTOCOL;
    rfNodes.splice(rfNodes.length > 0 ? 1 : 0, 0, {
      id: protocolId,
      type: 'protocol',
      position: { x: 300, y: 40 },
      data: {
        global: true,
        config,
        convertTo: null,
        channels: getEffectiveChannels(config, null),
        schema: schemaFromProtocolConfig(config),
        label: 'Protocol',
      },
      selected: false,
    } as Node);
  }

  // 边改写: 通道源 chN 出边 → Protocol 节点; loopbackIn → in
  const rfEdges: Edge[] = [];
  let hasTransportToProtocolByteEdge = false;
  for (const e of snap.rfEdges ?? []) {
    let source = e.source;
    let target = e.target;
    let targetHandle = e.targetHandle;
    if (source.startsWith(LEGACY_CHANNEL_SOURCE_PREFIX)) source = protocolId;
    if (target.startsWith(LEGACY_CHANNEL_SOURCE_PREFIX)) target = protocolId;
    if (targetHandle === 'loopbackIn') targetHandle = 'in';
    // 两端都指向已删除通道源以外的不存在节点时丢弃 (防御)
    if (source === target && source === protocolId && !e.sourceHandle?.startsWith('ch')) continue;
    if (source === transportId && e.sourceHandle === 'rx' && target === protocolId && targetHandle === 'in') {
      hasTransportToProtocolByteEdge = true;
    }
    rfEdges.push({ ...e, source, target, targetHandle });
  }
  if (!hasTransportToProtocolByteEdge) {
    rfEdges.unshift({
      id: 'migrated-transport-protocol',
      source: transportId,
      sourceHandle: 'rx',
      target: protocolId,
      targetHandle: 'in',
    });
  }

  const { transport: _t, protocol: _p, ...rest } = snap;
  // widgets 数组内的 Command 旧版单帧配置 → frames
  const widgets = snap.widgets?.map((w) =>
    isLegacyCommand(w) && w.kind === 'Command'
      ? { kind: 'Command' as const, params: normalizeCommandConfig(w.params) }
      : w
  );
  return { ...rest, version: 3, rfNodes, rfEdges, widgets };
}

/// 检测快照实际包含哪些分区 (供拆分备份导入时的勾选预填), 按 ALL_BACKUP_SECTIONS 顺序返回
/// v3: transportProtocol 分区内容 = rfNodes 中的全局 Transport/Protocol 节点
export function detectPresentSections(snap: AppSnapshot): BackupSection[] {
  const globalNodes = (snap.rfNodes ?? []).filter((n) => n.data?.global === true);
  const globalIds = new Set(globalNodes.map((n) => n.id));
  const widgetNodes = (snap.rfNodes ?? []).filter((n) => n.data?.global !== true);
  // 字节边 (两端皆全局节点) 属于 transportProtocol 分区, 不算 nodeGraph 内容
  const nonGlobalEdges = (snap.rfEdges ?? []).filter(
    (e) => !globalIds.has(e.source) || !globalIds.has(e.target)
  );
  const has = (s: BackupSection): boolean => {
    switch (s) {
      case 'settings': return snap.settings != null;
      case 'transportProtocol':
        return snap.protocol != null || snap.transport != null || globalNodes.length > 0;
      case 'widgetsTabs': return snap.widgets != null || snap.controlTabs != null || snap.dataTabs != null;
      case 'nodeGraph': return widgetNodes.length > 0 || nonGlobalEdges.length > 0 || (snap.rfNodes != null && globalNodes.length === 0);
      case 'windowLayout': return snap.dockRoot != null || snap.dockCards != null;
    }
  };
  return ALL_BACKUP_SECTIONS.filter(has);
}

// ==================== 状态恢复 ====================

/// 将 data 分类的缓存容量设置同步到后端与前端 buffer 实例
/// (与 settingsStore.applyDataCapacity 相同 — 该函数未导出, 此处复刻)
/// v3: 波形/原始数据容量按源 (节点) 生效, 对当前图中全部 Protocol/Transport 节点应用
function applyDataCapacity(settings: AppSettings) {
  const data = settings.data;
  const nodes = useAppStore.getState().rfNodes;
  for (const n of nodes) {
    if (n.data?.global !== true) continue;
    if (n.type === 'protocol') {
      api.setWaveformBufferCapacity(n.id, data.waveformBufferPoints).catch((e: unknown) =>
        console.warn('[appExport] 设置波形缓冲区容量失败:', e)
      );
    } else if (n.type === 'transport') {
      api.setRawDataBufferCapacity(n.id, data.rawDataBufferBytes).catch((e: unknown) =>
        console.warn('[appExport] 设置原始数据缓冲区容量失败:', e)
      );
    }
  }
  api.setCanBufferCapacity(data.canBufferFrames).catch((e: unknown) =>
    console.warn('[appExport] 设置 CAN 缓冲区容量失败:', e)
  );
  api.setLogicBufferCapacity(data.logicBufferSamples).catch((e: unknown) =>
    console.warn('[appExport] 设置逻辑缓冲区容量失败:', e)
  );
  setRawDataPreviewCapacity(data.rawDataBufferBytes);
  canFrameBuffer.setCapacity(data.canBufferFrames);
  logicSampleBuffer.setCapacity(data.logicBufferSamples);
}

/// 剥离 AI API key — 钥匙串之外的任何副本 (磁盘 / 备份文件) 都不携带密钥
function stripApiKey(settings: AppSettings): AppSettings {
  return { ...settings, ai: { ...settings.ai, apiKey: '' } };
}

/// 恢复设置到 settings store, 并同步应用到磁盘 / 主题 / 容量
async function applySettings(settings: AppSettings): Promise<void> {
  // 导入文件中的密钥 (旧版备份) 迁入钥匙串, 不再落盘
  if (settings.ai.apiKey) {
    try {
      await api.aiKeychainSet(settings.ai.adapter, settings.ai.apiKey);
    } catch (e) {
      console.warn('[appExport] 密钥迁入钥匙串失败:', e);
    }
  }
  useSettingsStore.setState({ settings });
  applyAppearance(settings.appearance);
  applyDataCapacity(settings);
  // 持久化到磁盘 (settings.json), 保证重启后仍生效
  try {
    const store = new LazyStore('settings.json');
    await store.set('app', stripApiKey(settings));
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
/// 传入前请先经 parseSnapshot / migrateSnapshotToV3 迁移为 v3
export async function applySnapshot(
  snap: AppSnapshot,
  opts?: { sections?: BackupSection[] }
): Promise<void> {
  const migrated = migrateSnapshotToV3(snap);
  const want = new Set(resolveSections(migrated, opts));
  // 应用前存在的 controlTabs — 应用后消失的 tab 需清理其后端残留图
  const prevTabIds = useAppStore.getState().controlTabs.map((t) => t.id);

  // 1. 设置
  if (want.has('settings') && migrated.settings) {
    await applySettings(migrated.settings);
  }

  // 2. 传输 + 协议 (v3: 全局节点; 未同时应用 nodeGraph 时按 id 合并进现有图)
  const applyNodeGraph = want.has('nodeGraph') && migrated.rfNodes != null;
  if (want.has('transportProtocol') && !applyNodeGraph && migrated.rfNodes) {
    const globalNodes = migrated.rfNodes.filter((n) => n.data?.global === true);
    if (globalNodes.length > 0) {
      const globalIds = new Set(globalNodes.map((n) => n.id));
      const byteEdges = (migrated.rfEdges ?? []).filter(
        (e) => globalIds.has(e.source) && globalIds.has(e.target)
      );
      useAppStore.setState((s) => ({
        rfNodes: [
          ...s.rfNodes.filter((n) => !globalIds.has(n.id)),
          ...globalNodes,
        ],
        rfEdges: [
          ...s.rfEdges.filter((e) => !(globalIds.has(e.source) && globalIds.has(e.target))),
          ...byteEdges,
        ],
      }));
    }
  }

  // 3. 控件 + 标签页 + 活动页 + RawData 视图偏好
  if (want.has('widgetsTabs')) {
    const patch: Record<string, unknown> = {};
    if (migrated.widgets) patch.widgets = migrated.widgets;
    if (migrated.controlTabs) patch.controlTabs = migrated.controlTabs;
    if (migrated.dataTabs) {
      // 兜底注入 fixed compile-errors / compile-results Tab
      // 老快照 (或外部导入的快照) 可能不包含; 缺一补一, 已存在则跳过
      const fixedTabs: DataTab[] = [
        { id: 'compile-errors-fixed', type: 'compile-errors', name: 'Compile Errors', closable: false },
        { id: 'compile-results-fixed', type: 'compile-results', name: 'Compile Results', closable: false },
        // 旧快照无属性面板 tab — 兜底补入 (默认布局卡片按 id 承载它)
        { id: 'node-properties-fixed', type: 'node-properties', name: 'Properties', closable: true },
      ];
      const have = new Set(migrated.dataTabs.map((t) => t.id));
      const mergedDataTabs = [...migrated.dataTabs];
      for (const ft of fixedTabs) {
        if (!have.has(ft.id)) mergedDataTabs.push(ft);
      }
      patch.dataTabs = mergedDataTabs;
    }
    if (migrated.activeDataTabId != null) {
      patch.activeDataTabId = migrated.activeDataTabId;
    } else {
      // 快照未指定活动数据 Tab → 兜底到 compile-results-fixed (须存在)
      const ids = (patch.dataTabs as DataTab[] | undefined)?.map((t) => t.id) ?? [];
      if (ids.includes('compile-results-fixed')) patch.activeDataTabId = 'compile-results-fixed';
    }
    if (migrated.activeControlTabId != null) patch.activeControlTabId = migrated.activeControlTabId;
    useAppStore.setState(patch);
    if (migrated.rawDataViewPrefs) {
      useRawDataViewStore.setState({
        prefsByWidget: migrated.rawDataViewPrefs as Record<string, RawDataViewPrefs>,
      });
    }
  }

  // 4. 节点图 (含全局 Transport/Protocol 节点)
  if (applyNodeGraph && migrated.rfNodes && migrated.rfEdges) {
    // 迁移旧版快照: 连到 RawData 的边可能还带着回退端口 'data' 作为 targetHandle,
    // 而 RawData 的端口是动态派生的 (`src:<source>:<handle>`) — 不归一化会导致
    // React Flow 找不到 handle (warning #008), 边无法渲染
    const rawDataNodeIds = new Set(
      migrated.rfNodes
        .filter((n) => (n.data?.widget as WidgetConfig | undefined)?.kind === 'RawData')
        .map((n) => n.id)
    );
    const rfEdges = migrated.rfEdges.map((e) =>
      rawDataNodeIds.has(e.target) && !e.targetHandle?.startsWith('src:')
        ? { ...e, targetHandle: rawDataPortId(e.source, e.sourceHandle) }
        : e
    );
    useAppStore.setState({ rfNodes: migrated.rfNodes, rfEdges });
  }

  // 5. 窗口组织
  if (want.has('windowLayout') && migrated.dockRoot && migrated.dockCards) {
    useDockStore.setState({
      root: migrated.dockRoot,
      cards: migrated.dockCards,
      focusedCardId: null,
    });
    if (migrated.sidebarDock) useLayoutStore.setState({ sidebarDock: migrated.sidebarDock });
  }

  // 6. 重新同步后端节点图 (节点图或标签页变化后)
  if (want.has('nodeGraph') || want.has('widgetsTabs') || want.has('transportProtocol')) {
    // 应用后消失的 tab: 后端仍残留其图与其名下的全局节点, 须显式移除。
    // 顺序: 先同步存活 tab (全局节点重新托管到存活 tab 名下), 再移除消失的 tab
    // (必须等同步落地后再 remove — 先后顺序即正确性本身)
    const syncs = useAppStore
      .getState()
      .controlTabs.map((tab) => useAppStore.getState().syncTabGraph(tab.id));
    void Promise.all(syncs).then(() => {
      const currentTabIds = new Set(useAppStore.getState().controlTabs.map((t) => t.id));
      for (const tabId of prevTabIds) {
        if (!currentTabIds.has(tabId)) useAppStore.getState().removeTabGraph(tabId);
      }
    });
  }

  // 7. 快照应用是批量覆盖而非用户单步操作 — 撤销历史重置为新基线
  rebaseHistory('opImportWorkspace');
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

    // 若当前有任一连接, 提示用户重新连接以应用导入的传输配置 (不自动连接)
    const isConnected = Object.values(useAppStore.getState().connectionStates).some((v) => v === 'Connected');
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
    const isConnected = Object.values(useAppStore.getState().connectionStates).some((v) => v === 'Connected');
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
