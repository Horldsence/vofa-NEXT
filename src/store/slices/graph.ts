import {
  applyNodeChanges,
  applyEdgeChanges,
  addEdge,
  type Node,
  type Edge,
  type NodeChange,
  type EdgeChange,
  type Connection,
} from '@xyflow/react';
import { nanoid } from 'nanoid';
import { api } from '../../lib/tauri/tauri';
import { setInputValue as apiSetInputValue, submitCustomOutput as apiSubmitCustomOutput, submitCustomTextOutput as apiSubmitCustomTextOutput } from '../../lib/buffers/graphSubscription';
import {
  createTransportNode,
  createProtocolNode,
  isGlobalNode,
  syncTabGraphToBackend,
  DEFAULT_PROTOCOL_CONFIG,
} from '../appStoreHelpers';
import { rawDataPortId } from '../../lib/utils/nodeDef';
import type { ProtocolConfig, TransportConfig, WidgetConfig } from '../../types';

export interface GraphSlice {
  rfNodes: Node[];
  rfEdges: Edge[];
  onNodesChange: (changes: NodeChange[]) => void;
  onEdgesChange: (changes: EdgeChange[]) => void;
  onConnect: (connection: Connection) => void;
  getTabNodes: (tabId: string) => Node[];
  getTabEdges: (tabId: string) => Edge[];
  syncTabGraph: (tabId: string) => void;
  syncAllTabGraphs: () => void;
  removeTabGraph: (tabId: string) => void;
  setInputValue: (widgetId: string, value: number) => void;
  submitCustomOutput: (widgetId: string, outputs: Record<string, number>) => void;
  submitCustomTextOutput: (widgetId: string, outputs: Record<string, string>) => void;
  /// 添加全局节点 (Transport/Protocol) — 渲染在所有 tab 画布上
  addTransportNode: (kind: TransportConfig['kind'], position?: { x: number; y: number }) => void;
  addProtocolNode: (config?: ProtocolConfig, position?: { x: number; y: number }) => void;
  removeGlobalNode: (nodeId: string) => void;
  /// 更新 Transport 节点配置 (节点 data + 全 tab 图同步)
  setTransportNodeConfig: (nodeId: string, config: TransportConfig) => void;
  /// 初始图种子: 设备(TestData) → 协议解析(JustFloat) → RawData — 新用户开箱即有完整数据通路
  seedInitialGraph: (rawDataWidgetId: string) => void;
}

export function createGraphSlice(set: any, get: any): GraphSlice {
  /// 变更涉及的 tab 集合 — 涉及全局节点时返回全部 tab
  const affectedTabsOf = (nodeIds: (string | null | undefined)[]): string[] => {
    const state = get();
    let touchesGlobal = false;
    const tabs = new Set<string>();
    for (const id of nodeIds) {
      if (!id) continue;
      const node = state.rfNodes.find((n: Node) => n.id === id);
      if (!node) continue;
      if (isGlobalNode(node)) touchesGlobal = true;
      else if (node.data?.tabId) tabs.add(node.data.tabId as string);
    }
    if (touchesGlobal) return state.controlTabs.map((t: any) => t.id as string);
    return [...tabs];
  };

  return {
    rfNodes: [],
    rfEdges: [],

    syncTabGraph: (tabId) => {
      void syncTabGraphToBackend(tabId);
    },

    syncAllTabGraphs: () => {
      get().controlTabs.forEach((tab: any) => get().syncTabGraph(tab.id));
    },

    removeTabGraph: (tabId) => {
      void api.removeTabGraph(tabId);
    },

    setInputValue: (widgetId, value) => {
      void apiSetInputValue(widgetId, value);
    },

    submitCustomOutput: (widgetId, outputs) => {
      void apiSubmitCustomOutput(widgetId, outputs);
    },

    submitCustomTextOutput: (widgetId, outputs) => {
      void apiSubmitCustomTextOutput(widgetId, outputs);
    },

    addTransportNode: (kind, position) => {
      const node = createTransportNode(kind, position);
      set((s: any) => ({ rfNodes: [...s.rfNodes, node] }));
      get().syncAllTabGraphs();
    },

    addProtocolNode: (config, position) => {
      const node = createProtocolNode(config, position);
      set((s: any) => ({ rfNodes: [...s.rfNodes, node] }));
      get().syncAllTabGraphs();
      get().ensureChannelsPolling?.();
    },

    removeGlobalNode: (nodeId) => {
      const node = get().rfNodes.find((n: Node) => n.id === nodeId);
      if (!node || !isGlobalNode(node)) return;
      // 关闭仍打开的连接 (尽力而为)
      if (node.type === 'transport') void api.closeTransport(nodeId).catch(() => {});
      set((s: any) => ({
        rfNodes: s.rfNodes.filter((n: Node) => n.id !== nodeId),
        rfEdges: s.rfEdges.filter((e: Edge) => e.source !== nodeId && e.target !== nodeId),
      }));
      get().syncAllTabGraphs();
      get().ensureChannelsPolling?.();
    },

    setTransportNodeConfig: (nodeId, config) => {
      set((s: any) => ({
        rfNodes: s.rfNodes.map((n: Node) =>
          n.id === nodeId && n.type === 'transport'
            ? { ...n, data: { ...n.data, config, label: config.kind } }
            : n
        ),
      }));
      get().syncAllTabGraphs();
    },

    seedInitialGraph: (rawDataWidgetId) => {
      // 默认设备选 TestData — 新用户无硬件也能连接后立即看到数据
      const transport = createTransportNode('TestData', { x: 60, y: 100 });
      const protocol = createProtocolNode(DEFAULT_PROTOCOL_CONFIG, { x: 300, y: 100 });
      set((s: any) => ({ rfNodes: [...s.rfNodes, transport, protocol] }));
      // onConnect 负责 RawData 动态端口改写 (rawDataPortId) 与图同步
      get().onConnect({ source: transport.id, sourceHandle: 'rx', target: protocol.id, targetHandle: 'in' });
      get().onConnect({ source: protocol.id, sourceHandle: 'out', target: rawDataWidgetId, targetHandle: 'data' });
      get().ensureChannelsPolling?.();
    },

    onNodesChange: (changes) => {
      // 键盘 Delete 删除全局节点: 清理其边 + 关闭连接 + 全 tab 重同步
      // (X 按钮走 removeGlobalNode; 这里兜 React Flow 的 remove change)
      const removedGlobalIds: string[] = [];
      const removedTransportIds: string[] = [];
      // 被删除的 widget 节点所属 tab — 删除后其边/节点定义须同步移除
      const removedWidgetTabIds = new Set<string>();
      for (const ch of changes) {
        if (ch.type === 'remove') {
          const node = get().rfNodes.find((n: Node) => n.id === ch.id);
          if (node && isGlobalNode(node)) {
            removedGlobalIds.push(ch.id);
            if (node.type === 'transport') removedTransportIds.push(ch.id);
          } else if (node?.data?.tabId) {
            removedWidgetTabIds.add(node.data.tabId as string);
          }
        }
      }
      set((s: any) => ({
        rfNodes: applyNodeChanges(changes, s.rfNodes),
        rfEdges: removedGlobalIds.length
          ? s.rfEdges.filter((e: Edge) => !removedGlobalIds.includes(e.source) && !removedGlobalIds.includes(e.target))
          : s.rfEdges,
      }));
      if (removedGlobalIds.length) {
        for (const id of removedTransportIds) {
          void api.closeTransport(id).catch(() => {});
        }
        get().syncAllTabGraphs();
        get().ensureChannelsPolling?.();
      }
      removedWidgetTabIds.forEach((tabId) => get().syncTabGraph(tabId));
    },

    onEdgesChange: (changes) => {
      const affected = new Set<string>();
      // remove change 只有 id (无 source/target) — 必须先在旧边集合里查出端点
      const oldEdges = get().rfEdges;
      for (const ch of changes) {
        let source: string | null | undefined;
        let target: string | null | undefined;
        if (ch.type === 'remove') {
          const edge = oldEdges.find((e: Edge) => e.id === ch.id);
          source = edge?.source;
          target = edge?.target;
        } else {
          if ('source' in ch) source = (ch as any).source;
          if ('target' in ch) target = (ch as any).target;
        }
        if (source) affectedTabsOf([source]).forEach((t) => affected.add(t));
        if (target) affectedTabsOf([target]).forEach((t) => affected.add(t));
      }
      set((s: any) => ({
        rfEdges: applyEdgeChanges(changes, s.rfEdges),
      }));
      affected.forEach((tabId) => get().syncTabGraph(tabId));
    },

    onConnect: (connection) => {
      const newEdge: Edge = {
        ...connection,
        id: nanoid(8),
      };
      const sourceNode = get().rfNodes.find((n: Node) => n.id === connection.source);
      const targetNode = get().rfNodes.find((n: Node) => n.id === connection.target);
      // tabId 推断: widget 节点归属其 tab; 全局节点参与的边归属当前活跃 tab
      let tabId: string | undefined =
        (sourceNode && !isGlobalNode(sourceNode) ? (sourceNode.data?.tabId as string) : undefined) ??
        (targetNode && !isGlobalNode(targetNode) ? (targetNode.data?.tabId as string) : undefined);
      if (!tabId) tabId = get().activeControlTabId;
      // RawData 输入端口是动态派生的 (`src:<source>:<handle>`), 连接时它显示的回退端口 'data'
      // 在入边建立后即消失 — 必须把边的 targetHandle 改写为派生端口 id,
      // 否则 React Flow 找不到 handle (warning #008), 边无法渲染
      const targetWidget = targetNode?.data?.widget as WidgetConfig | undefined;
      if (targetWidget?.kind === 'RawData') {
        newEdge.targetHandle = rawDataPortId(connection.source, connection.sourceHandle);
      }
      set((s: any) => ({
        rfEdges: addEdge(newEdge, s.rfEdges),
      }));
      if (tabId) get().syncTabGraph(tabId);
    },

    getTabNodes: (tabId) => {
      const { rfNodes } = get();
      return rfNodes.filter((n: Node) => n.data.tabId === tabId || isGlobalNode(n));
    },

    getTabEdges: (tabId) => {
      const tabNodeIds = new Set(get().getTabNodes(tabId).map((n: Node) => n.id));
      return get().rfEdges.filter((e: Edge) => tabNodeIds.has(e.source) && tabNodeIds.has(e.target));
    },
  };
}
