//! 应用快速开始模板 — 替换 / 合并两种模式
//!
//! - 替换: 清空当前工作区 (节点图 + 窗口组织 + 传输/协议 + 控件/标签页) 后应用模板,
//!          保留用户设置 (语言/主题等)。
//! - 合并: 把模板作为新控件标签页追加进当前工作区, 所有 ID 重映射避免冲突,
//!          传输/协议为全局单例, 合并模式下不改变。

import { nanoid } from 'nanoid';
import { type Node, type Edge } from '@xyflow/react';
import { useAppStore, CHANNEL_SOURCE_ID } from '../../store/appStore';
import { useDockStore } from '../../store/dockStore';
import { applySnapshot, type AppSnapshot } from '../tauri/appExport';
import { notify, formatError } from '../tauri/notifications';
import { t } from '../../i18n';
import type { WidgetConfig, DataTab } from '../../types';

export type TemplateApplyMode = 'replace' | 'merge';

/// 重映射 RawData 动态端口 id (`src:<sourceId>:<handle>`) 中的 sourceId
function remapRawDataHandle(handle: string | undefined | null, idMap: Map<string, string>): string | undefined | null {
  if (!handle || !handle.startsWith('src:')) return handle;
  const m = /^src:([^:]+):(.*)$/.exec(handle);
  if (!m) return handle;
  const newSource = idMap.get(m[1]) ?? m[1];
  return `src:${newSource}:${m[2]}`;
}

/// 合并模式: 将模板作为新控件标签页追加
function mergeTemplate(snap: AppSnapshot): void {
  const app = useAppStore.getState();
  const newTabId = `tpl-${nanoid(6)}`;

  // widget 旧 id → 新 id
  const idMap = new Map<string, string>();
  const widgets = (snap.widgets ?? []).map((w) => {
    const newId = nanoid(8);
    idMap.set(w.params.id, newId);
    return { ...w, params: { ...w.params, id: newId } } as WidgetConfig;
  });

  // 模板的单个控件标签页 → 新标签页
  const templateTab = snap.controlTabs?.[0];
  const newControlTab = {
    id: newTabId,
    name: templateTab?.name ?? 'Template',
    widgets: (templateTab?.widgets ?? []).map((oldId) => idMap.get(oldId) ?? oldId),
  };

  // 数据标签页: 跳过全局固定的 waveform-fixed, 其余重映射
  const existingDataIds = new Set(app.dataTabs.map((t) => t.id));
  const newDataTabs: DataTab[] = [];
  for (const tab of snap.dataTabs ?? []) {
    if (tab.id === 'waveform-fixed') continue;
    let newId = nanoid(8);
    while (existingDataIds.has(newId)) newId = nanoid(8);
    const widgetId = tab.widgetId ? (idMap.get(tab.widgetId) ?? tab.widgetId) : undefined;
    newDataTabs.push({ ...tab, id: newId, ...(widgetId ? { widgetId } : {}) });
  }

  // 节点: 通道源 → 新标签页; 控件节点 → 重映射 id 与 data.widget.params.id
  const sourceNodeId = `${CHANNEL_SOURCE_ID}-${newTabId}`;
  const rfNodes: Node[] = (snap.rfNodes ?? []).map((n) => {
    if (n.type === 'channelSource') {
      return { ...n, id: sourceNodeId, data: { ...n.data, tabId: newTabId } };
    }
    const newId = idMap.get(n.id) ?? nanoid(8);
    const w = n.data.widget as WidgetConfig | undefined;
    return {
      ...n,
      id: newId,
      data: {
        ...n.data,
        tabId: newTabId,
        widget: w ? { ...w, params: { ...w.params, id: newId } } : w,
      },
    };
  });

  // 边: source/target 重映射, RawData targetHandle 端口 id 一并重映射
  const rfEdges: Edge[] = (snap.rfEdges ?? []).map((e) => {
    const source = e.source.startsWith(CHANNEL_SOURCE_ID)
      ? sourceNodeId
      : (idMap.get(e.source) ?? e.source);
    const target = idMap.get(e.target) ?? e.target;
    return {
      ...e,
      id: nanoid(8),
      source,
      target,
      targetHandle: remapRawDataHandle(e.targetHandle, idMap),
    };
  });

  useAppStore.setState((s) => ({
    widgets: [...s.widgets, ...widgets],
    controlTabs: [...s.controlTabs, newControlTab],
    dataTabs: [...s.dataTabs, ...newDataTabs],
    rfNodes: [...s.rfNodes, ...rfNodes],
    rfEdges: [...s.rfEdges, ...rfEdges],
    activeControlTabId: newTabId,
  }));

  // 同步新标签页节点图到后端 + 对账 Dock 卡片 (安置新增 Tab)
  useAppStore.getState().syncTabGraph(newTabId);
  const st = useAppStore.getState();
  useDockStore.getState().reconcile('control', st.controlTabs.map((t) => t.id));
  useDockStore.getState().reconcile('data', st.dataTabs.map((t) => t.id));
}

/// 应用模板 (替换 / 合并), 完成后弹出成功提示
export async function applyTemplate(
  snap: AppSnapshot,
  mode: TemplateApplyMode
): Promise<void> {
  const lang = useAppStore.getState().lang;
  try {
    if (mode === 'replace') {
      // 模板不含设置 — 明确只应用节点图/窗口组织/传输协议/控件标签页
      await applySnapshot(snap, {
        sections: ['nodeGraph', 'windowLayout', 'transportProtocol', 'widgetsTabs'],
      });
    } else {
      mergeTemplate(snap);
    }

    const isConnected = useAppStore.getState().connectionState === 'Connected';
    notify.info(
      t(lang, 'templateApplySuccess'),
      isConnected ? t(lang, 'templateApplySuccessDescReconnect') : t(lang, 'templateApplySuccessDesc'),
      { source: 'applyTemplate' }
    );
  } catch (e) {
    notify.error(t(lang, 'templateApplyFailed'), formatError(e), { source: 'applyTemplate' });
  }
}
