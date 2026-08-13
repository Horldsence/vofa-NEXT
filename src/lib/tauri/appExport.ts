//! 全应用配置导出 / 导入 (单个 JSON 文件, 通过系统文件对话框)
//!
//! 备份范围: 设置 + 协议 + 传输 + 控件 + 节点图 + 数据标签页 + RawData 视图偏好
//! 用于备份 / 恢复 / 迁移到另一台机器。

import { save, open } from '@tauri-apps/plugin-dialog';
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
import { LazyStore } from '@tauri-apps/plugin-store';
import type { Node, Edge } from '@xyflow/react';
import { useAppStore } from '../../store/appStore';
import { useSettingsStore } from '../../store/settingsStore';
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

/// 备份快照 schema — version 1
export interface AppSnapshot {
  version: 1;
  exportedAt: string;
  settings: AppSettings;
  protocol: ProtocolConfig;
  transport: TransportConfig;
  widgets: WidgetConfig[];
  controlTabs: ControlTab[];
  dataTabs: DataTab[];
  activeDataTabId: string;
  activeControlTabId: string;
  rfNodes: Node[];
  rfEdges: Edge[];
  rawDataViewPrefs: Record<string, unknown>;
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

// ==================== 快照收集 / 序列化 / 解析 ====================

/// 读取所有 store 的当前状态并生成快照。
/// rfNodes/rfEdges 经 JSON 往返确保无函数 / undefined 等不可序列化字段。
export function collectSnapshot(): AppSnapshot {
  const app = useAppStore.getState();
  return {
    version: 1,
    exportedAt: new Date().toISOString(),
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
  };
}

export function serializeSnapshot(snap: AppSnapshot): string {
  return JSON.stringify(snap, null, 2);
}

/// 解析备份 JSON 并做最小校验, 非法时抛出带清晰信息的 Error
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
  if (data.version !== 1) {
    throw new Error(`不支持的备份版本: ${String(data.version)}`);
  }
  if (
    !Array.isArray(data.widgets) ||
    !Array.isArray(data.controlTabs) ||
    !Array.isArray(data.dataTabs) ||
    !Array.isArray(data.rfNodes) ||
    !Array.isArray(data.rfEdges)
  ) {
    throw new Error('备份文件缺少必要字段 (widgets / tabs / nodes / edges)');
  }
  if (!data.settings || !data.protocol || !data.transport) {
    throw new Error('备份文件缺少设置 / 协议 / 传输配置');
  }
  if (typeof data.activeDataTabId !== 'string' || typeof data.activeControlTabId !== 'string') {
    throw new Error('备份文件缺少活动标签页信息');
  }
  return data as AppSnapshot;
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

/// 将快照应用到所有 store (恢复)
export async function applySnapshot(snap: AppSnapshot): Promise<void> {
  const app = useAppStore.getState();

  // 1. 设置
  await applySettings(snap.settings);

  // 2. 协议 (同步到后端)
  await app.setProtocolConfig(snap.protocol);

  // 3. 传输
  app.setTransportConfig(snap.transport);

  // 4. 控件 + 标签页 + 活动页
  useAppStore.setState({
    widgets: snap.widgets,
    controlTabs: snap.controlTabs,
    dataTabs: snap.dataTabs,
    activeDataTabId: snap.activeDataTabId,
    activeControlTabId: snap.activeControlTabId,
  });

  // 5. 节点图
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

  // 6. 重新同步后端节点图
  for (const tab of snap.controlTabs) {
    useAppStore.getState().syncTabGraph(tab.id);
  }

  // 7. RawData 视图偏好
  useRawDataViewStore.setState({
    prefsByWidget: snap.rawDataViewPrefs as Record<string, RawDataViewPrefs>,
  });
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
    const selected = await open({
      multiple: false,
      directory: false,
      filters: JSON_FILTERS,
    });
    // 用户取消
    if (!selected) return;
    const path = Array.isArray(selected) ? selected[0] : selected;
    if (!path) return;

    const json = await readTextFile(path);
    const snap = parseSnapshot(json);
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
