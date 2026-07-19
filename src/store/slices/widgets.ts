import type { Node } from '@xyflow/react';
import type { WidgetConfig } from '../../types';
import { createWidget } from '../../lib/createWidget';

export interface WidgetSlice {
  widgets: WidgetConfig[];
  customEditorState: { open: boolean; widgetId: string | null };
  openCustomEditor: (widgetId?: string) => void;
  closeCustomEditor: () => void;
  addWidget: (widget: WidgetConfig, tabId: string, position?: { x: number; y: number }) => void;
  removeWidget: (id: string) => void;
  updateWidget: (id: string, widget: WidgetConfig) => void;
}

export function createWidgetSlice(set: any, get: any): WidgetSlice {
  return {
    widgets: [],
    customEditorState: { open: false, widgetId: null },

    openCustomEditor: (widgetId) =>
      set((s: any) => {
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
            controlTabs: s.controlTabs.map((t: any) =>
              t.id === tabId ? { ...t, widgets: [...t.widgets, widget.params.id] } : t
            ),
            customEditorState: { open: true, widgetId: widget.params.id },
          };
        }
        return { customEditorState: { open: true, widgetId } };
      }),

    closeCustomEditor: () => set({ customEditorState: { open: false, widgetId: null } }),

    addWidget: (widget, tabId, position) => {
      set((s: any) => {
        const pos = position ?? { x: 240 + Math.random() * 100, y: 80 + Math.random() * 80 };
        const newNode: Node = {
          id: widget.params.id,
          type: 'widget',
          position: pos,
          data: { widget, tabId },
        };
        const newState: Record<string, any> = {
          widgets: [...s.widgets, widget],
          rfNodes: [...s.rfNodes, newNode],
        };
        if (
          widget.kind === 'Waveform' ||
          widget.kind === 'PieChart' ||
          widget.kind === 'Image' ||
          widget.kind === 'Model3D' ||
          widget.kind === 'Spectrum' ||
          widget.kind === 'Command' ||
          widget.kind === 'FrameDecoder'
        ) {
          const tabType =
            widget.kind === 'Waveform'
              ? 'waveform-extra'
              : widget.kind === 'PieChart'
              ? 'pie'
              : widget.kind === 'Image'
              ? 'image'
              : widget.kind === 'Model3D'
              ? 'model3d'
              : widget.kind === 'Spectrum'
              ? 'spectrum'
              : widget.kind === 'Command'
              ? 'command'
              : 'frame-decoder';
          const tabName =
            widget.kind === 'Waveform'
              ? 'Waveform'
              : widget.params.label;
          const newTab = {
            id: widget.params.id,
            type: tabType,
            name: tabName,
            widgetId: widget.params.id,
            closable: true,
          };
          newState.dataTabs = [...s.dataTabs, newTab];
          newState.activeDataTabId = widget.params.id;
        }
        newState.controlTabs = s.controlTabs.map((t: any) =>
          t.id === tabId ? { ...t, widgets: [...t.widgets, widget.params.id] } : t
        );
        return newState;
      });
      get().syncTabGraph(tabId);
    },

    removeWidget: (id) => {
      const widget = get().widgets.find((w: WidgetConfig) => w.params.id === id);
      const affectedTabs = new Set<string>();
      const node = get().rfNodes.find((n: any) => n.id === id);
      if (node?.data?.tabId) affectedTabs.add(node.data.tabId as string);
      set((s: any) => {
        const newState: Record<string, any> = {
          widgets: s.widgets.filter((w: WidgetConfig) => w.params.id !== id),
          rfNodes: s.rfNodes.filter((n: any) => n.id !== id),
          rfEdges: s.rfEdges.filter((e: any) => e.source !== id && e.target !== id),
        };
        if (
          widget &&
          (widget.kind === 'Waveform' ||
            widget.kind === 'PieChart' ||
            widget.kind === 'Image')
        ) {
          const remaining = s.dataTabs.filter((t: any) => t.id !== id);
          newState.dataTabs = remaining;
          if (s.activeDataTabId === id) {
            newState.activeDataTabId = remaining[0]?.id ?? 'waveform-fixed';
          }
        }
        newState.controlTabs = s.controlTabs.map((t: any) => ({
          ...t,
          widgets: t.widgets.filter((w: string) => w !== id),
        }));
        return newState;
      });
      affectedTabs.forEach((tabId) => get().syncTabGraph(tabId));
    },

    updateWidget: (id, widget) => {
      const node = get().rfNodes.find((n: any) => n.id === id);
      const tabId = node?.data?.tabId as string | undefined;
      set((s: any) => ({
        widgets: s.widgets.map((w: WidgetConfig) => (w.params.id === id ? widget : w)),
        rfNodes: s.rfNodes.map((n: any) =>
          n.id === id ? { ...n, data: { ...n.data, widget } } : n
        ),
      }));
      if (tabId) get().syncTabGraph(tabId);
    },
  };
}
