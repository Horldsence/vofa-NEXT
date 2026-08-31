import type { Node } from '@xyflow/react';
import type { WidgetConfig } from '../../types';
import type { AppSlice } from './types';
import { createWidget, normalizeModel3DConfig } from '../../lib/utils/createWidget';
import { widgetToTab } from '../../lib/utils/widgetTab';
import { normalizeCommandConfig } from '../../lib/utils/commandFrames';
import { withHistoryOp, widgetKindLabelKey } from '../historyStore';
import type { HistoryTarget } from '../historyStore';
import type { AppStore } from '../appStore';

/** 控件类操作的目标语义 (行首徽章按画布同款分类色渲染) */
const widgetTarget = (kind: WidgetConfig['kind']): HistoryTarget => ({
  kind: 'node',
  node: { kind: 'widget', widgetKind: kind },
});

/// 控件配置归一化入口
/// - Command: 旧版单帧 → frames
/// - Model3D: 旧版缺 modelSource 字段 → builtin-cube fallback
/// 其余控件原样返回
function normalizeWidget(widget: WidgetConfig): WidgetConfig {
  if (widget.kind === 'Command') {
    return { kind: 'Command', params: normalizeCommandConfig(widget.params) };
  }
  if (widget.kind === 'Model3D') {
    return { kind: 'Model3D', params: normalizeModel3DConfig(widget.params) };
  }
  return widget;
}

export interface WidgetSlice {
  widgets: WidgetConfig[];
  customEditorState: { open: boolean; widgetId: string | null };
  openCustomEditor: (widgetId?: string) => void;
  closeCustomEditor: () => void;
  addWidget: (widget: WidgetConfig, tabId: string, position?: { x: number; y: number }) => void;
  removeWidget: (id: string) => void;
  updateWidget: (id: string, widget: WidgetConfig) => void;
}

export const createWidgetSlice: AppSlice<WidgetSlice> = (set, get) => {
  return {
    widgets: [],
    customEditorState: { open: false, widgetId: null },

    openCustomEditor: (widgetId) =>
      withHistoryOp({ opKey: 'opAddWidget', target: widgetTarget('Custom') }, () =>
        set((s) => {
          if (!widgetId) {
            const widget = createWidget('Custom');
            const tabId = s.activeControlTabId;
            const pos = { x: 280, y: 80 + Math.random() * 100 };
            const newNode: Node = {
              id: widget.params.id,
              type: 'widget',
              position: pos,
              data: { widget, tabId },
            };
            return {
              widgets: [...s.widgets, widget],
              rfNodes: [...s.rfNodes, newNode],
              controlTabs: s.controlTabs.map((t) =>
                t.id === tabId ? { ...t, widgets: [...t.widgets, widget.params.id] } : t
              ),
              customEditorState: { open: true, widgetId: widget.params.id },
            };
          }
          return { customEditorState: { open: true, widgetId } };
        })
      ),

    closeCustomEditor: () => set({ customEditorState: { open: false, widgetId: null } }),

    addWidget: (widget, tabId, position) =>
      withHistoryOp(
        {
          opKey: 'opAddWidget',
          detailKey: widgetKindLabelKey(widget.kind) ?? undefined,
          target: widgetTarget(widget.kind),
        },
        () => {
          widget = normalizeWidget(widget);
          set((s) => {
            const pos = position ?? { x: 240 + Math.random() * 100, y: 80 + Math.random() * 80 };
            const newNode: Node = {
              id: widget.params.id,
              type: 'widget',
              position: pos,
              data: { widget, tabId },
            };
            const newState: Partial<AppStore> = {
              widgets: [...s.widgets, widget],
              rfNodes: [...s.rfNodes, newNode],
            };
            // 有窗口的控件: 自动创建数据窗口 Tab (关闭窗口不会删除节点, 可双击节点重开)
            const tab = widgetToTab(widget);
            if (tab) {
              newState.dataTabs = [...s.dataTabs, tab];
              newState.activeDataTabId = widget.params.id;
            }
            newState.controlTabs = s.controlTabs.map((t) =>
              t.id === tabId ? { ...t, widgets: [...t.widgets, widget.params.id] } : t
            );
            return newState;
          });
          get().syncTabGraph(tabId);
        }
      ),

    removeWidget: (id) => {
      const widget = get().widgets.find((w: WidgetConfig) => w.params.id === id);
      const affectedTabs = new Set<string>();
      const node = get().rfNodes.find((n) => n.id === id);
      if (node?.data?.tabId) affectedTabs.add(node.data.tabId as string);
      withHistoryOp(
        {
          opKey: 'opRemoveWidget',
          detailKey: widget ? widgetKindLabelKey(widget.kind) ?? undefined : undefined,
          target: widget ? widgetTarget(widget.kind) : { kind: 'nodes' },
        },
        () => {
          set((s) => {
            const newState: Partial<AppStore> = {
              widgets: s.widgets.filter((w: WidgetConfig) => w.params.id !== id),
              rfNodes: s.rfNodes.filter((n) => n.id !== id),
              rfEdges: s.rfEdges.filter((e) => e.source !== id && e.target !== id),
            };
            if (
              widget &&
              (widget.kind === 'Waveform' ||
                widget.kind === 'PieChart' ||
                widget.kind === 'Image' ||
                widget.kind === 'RawData')
            ) {
              const remaining = s.dataTabs.filter((t) => t.id !== id);
              newState.dataTabs = remaining;
              if (s.activeDataTabId === id) {
                newState.activeDataTabId = remaining[0]?.id ?? 'waveform-fixed';
              }
            }
            newState.controlTabs = s.controlTabs.map((t) => ({
              ...t,
              widgets: t.widgets.filter((w: string) => w !== id),
            }));
            return newState;
          });
          affectedTabs.forEach((tabId) => get().syncTabGraph(tabId));
        }
      );
    },

    updateWidget: (id, widget) =>
      withHistoryOp(
        {
          opKey: 'opUpdateWidgetParams',
          detailKey: widgetKindLabelKey(widget.kind) ?? undefined,
          target: widgetTarget(widget.kind),
        },
        () => {
          widget = normalizeWidget(widget);
          const node = get().rfNodes.find((n) => n.id === id);
          const tabId = node?.data?.tabId as string | undefined;
          set((s) => ({
            widgets: s.widgets.map((w: WidgetConfig) => (w.params.id === id ? widget : w)),
            rfNodes: s.rfNodes.map((n) =>
              n.id === id ? { ...n, data: { ...n.data, widget } } : n
            ),
          }));
          if (tabId) get().syncTabGraph(tabId);
        },
        // 滑块拖拽等高频连续更新 — 同控件短窗内合并为一条
        { coalesceKey: `widget.params.${id}` }
      ),
  };
}
