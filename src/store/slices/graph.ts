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
import { useAppStore } from '../appStore';
import { withHistoryOp } from '../historyStore';
import type { HistoryTarget, NodeOpRef } from '../historyStore';
import { nodeLabelOf, nodeRefOf } from '../../lib/utils/nodeKindVisuals';
import type { ProtocolConfig, TransportConfig, WidgetConfig } from '../../types';

export interface GraphSlice {
  rfNodes: Node[];
  rfEdges: Edge[];
  /// 后端全局图版本号 (null = 尚未同步过; 作为下次提交的 base_version 冲突基线)
  graphVersion: number | null;
  /// 启动水合已完成 (workspace_get 已裁决: 水合或默认启动)
  workspaceReady: boolean;
  /// 是否水合了后端持久化工作区 (true = 不走初始同步与种子流程)
  workspaceRestored: boolean;
  setGraphVersion: (v: number) => void;
  onNodesChange: (changes: NodeChange[]) => void;
  onEdgesChange: (changes: EdgeChange[]) => void;
  onConnect: (connection: Connection) => void;
  getTabNodes: (tabId: string) => Node[];
  getTabEdges: (tabId: string) => Edge[];
  /// 同步指定 tab 图到后端 (同 tab 串行化); 返回错误文案, 成功 undefined
  syncTabGraph: (tabId: string) => Promise<string | undefined>;
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
  /** 节点 id → 视觉引用 (传输/协议/控件), 找不到返回 null */
  const nodeById = (id: string | null | undefined): Node | undefined =>
    get().rfNodes.find((n: Node) => n.id === id);
  const refOf = (id: string | null | undefined): NodeOpRef | null => nodeRefOf(nodeById(id));

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
    graphVersion: null,
    workspaceReady: false,
    workspaceRestored: false,

    setGraphVersion: (v) => set({ graphVersion: v }),

    syncTabGraph: (tabId) => syncTabGraphToBackend(tabId),

    syncAllTabGraphs: () => {
      get().controlTabs.forEach((tab: any) => get().syncTabGraph(tab.id));
    },

    removeTabGraph: (tabId) => {
      void (async () => {
        try {
          const derived = await api.removeTabGraph(tabId);
          if (derived?.nodes) useAppStore.getState().setDerived(derived.nodes);
        } catch {
          // 错误由 update_tab_graph 失败处理统一覆盖; 此处不打扰用户
        }
      })();
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

    addTransportNode: (kind, position) =>
      withHistoryOp(
        { opKey: 'opAddTransportNode', detailText: kind, target: { kind: 'node', node: { kind: 'transport' } } },
        () => {
        const node = createTransportNode(kind, position);
        set((s: any) => ({ rfNodes: [...s.rfNodes, node] }));
        get().syncAllTabGraphs();
      }),

    addProtocolNode: (config, position) =>
      withHistoryOp(
        {
          opKey: 'opAddProtocolNode',
          detailText: config?.kind ?? undefined,
          target: { kind: 'node', node: { kind: 'protocol' } },
        },
        () => {
        const node = createProtocolNode(config, position);
        set((s: any) => ({ rfNodes: [...s.rfNodes, node] }));
        get().syncAllTabGraphs();
      }),

    removeGlobalNode: (nodeId) => {
      const victim = get().rfNodes.find((n: Node) => n.id === nodeId);
      const ref = nodeRefOf(victim);
      return withHistoryOp(
        {
          opKey: 'opRemoveGlobalNode',
          detailText: nodeLabelOf(victim),
          target: ref ? { kind: 'node', node: ref } : undefined,
        },
        () => {
          const node = get().rfNodes.find((n: Node) => n.id === nodeId);
          if (!node || !isGlobalNode(node)) return;
          // 关闭仍打开的连接 (尽力而为)
          if (node.type === 'transport') void api.closeTransport(nodeId).catch(() => {});
          set((s: any) => ({
            rfNodes: s.rfNodes.filter((n: Node) => n.id !== nodeId),
            rfEdges: s.rfEdges.filter((e: Edge) => e.source !== nodeId && e.target !== nodeId),
          }));
          // 同步清理派生端口表 (后端 remove_tab_graph 也会发新的 GraphDerived,
          // 但全局节点删除不会触发 remove_tab_graph, 需前端本地清理)
          get().removeDerived([nodeId]);
          get().syncAllTabGraphs();
        }
      );
    },

    setTransportNodeConfig: (nodeId, config) =>
      withHistoryOp(
        {
          opKey: 'opUpdateTransportConfig',
          detailText: config.kind,
          target: { kind: 'node', node: { kind: 'transport' } },
        },
        () => {
          set((s: any) => ({
            rfNodes: s.rfNodes.map((n: Node) =>
              n.id === nodeId && n.type === 'transport'
                ? { ...n, data: { ...n.data, config, label: config.kind } }
                : n
            ),
          }));
          get().syncAllTabGraphs();
        },
        // 配置面板连续调节 (端口名/波特率输入等) — 短窗合并
        { coalesceKey: `transport.config.${nodeId}` }
      ),

    seedInitialGraph: (rawDataWidgetId) => {
      // 默认设备选 TestData — 新用户无硬件也能连接后立即看到数据
      const transport = createTransportNode('TestData', { x: 60, y: 100 });
      const protocol = createProtocolNode(DEFAULT_PROTOCOL_CONFIG, { x: 300, y: 100 });
      set((s: any) => ({ rfNodes: [...s.rfNodes, transport, protocol] }));
      // onConnect 负责 RawData 动态端口改写 (rawDataPortId) 与图同步
      get().onConnect({ source: transport.id, sourceHandle: 'rx', target: protocol.id, targetHandle: 'in' });
      get().onConnect({ source: protocol.id, sourceHandle: 'out', target: rawDataWidgetId, targetHandle: 'data' });
    },

    onNodesChange: (changes) => {
      // 键盘 Delete 删除全局节点: 清理其边 + 关闭连接 + 全 tab 重同步
      // (X 按钮走 removeGlobalNode; 这里兜 React Flow 的 remove change)
      // 撤销埋点: 仅记录 remove / position 两类 change, select/dimensions 不入历史;
      // 拖动期间高频 position 批按时间窗合并为一条「移动节点」
      const removing = changes.some((ch) => ch.type === 'remove');
      const moving = changes.some((ch) => ch.type === 'position');
      const applyChanges = () => {
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
        // 拖拽结束 (dragging=false 的收尾批) 上报最终位置 — 画布位置的后端
        // 权威存储随工作区落盘, 重启后布局不回跳; 拖拽过程批 (dragging=true) 不发
        if (moving) {
          const finalPos: Record<string, { x: number; y: number }> = {};
          for (const ch of changes) {
            if (ch.type !== 'position' || ch.dragging === true) continue;
            const node = get().rfNodes.find((n: Node) => n.id === ch.id);
            if (node) finalPos[ch.id] = { x: node.position.x, y: node.position.y };
          }
          if (Object.keys(finalPos).length) {
            void api.setNodePositions(finalPos).catch(() => {});
          }
        }
        // 同步清理被删节点的派生端口表
        if (removedGlobalIds.length) {
          get().removeDerived(removedGlobalIds);
          for (const id of removedTransportIds) {
            void api.closeTransport(id).catch(() => {});
          }
          get().syncAllTabGraphs();
        }
        removedWidgetTabIds.forEach((tabId) => get().syncTabGraph(tabId));
      };
      if (removing || moving) {
        // 删除型批: 取首个被删节点作为徽章归属; 移动/混合删除为中性
        let target: HistoryTarget = { kind: 'nodes' };
        if (removing) {
          const removedId = changes.find(
            (ch): ch is NodeChange & { id: string } => ch.type === 'remove'
          )?.id;
          const ref = refOf(removedId);
          if (ref) target = { kind: 'node', node: ref };
        }
        withHistoryOp(
          removing ? { opKey: 'opRemoveNodes', target } : { opKey: 'opMoveNodes', target },
          applyChanges,
          { coalesceKey: removing ? 'node.remove' : 'node.move' }
        );
      } else {
        applyChanges();
      }
    },

    onEdgesChange: (changes) => {
      // 撤销埋点: 仅记录 remove change (选择变化不入历史), 连发合并为一条
      const removing = changes.some((ch) => ch.type === 'remove');
      const applyChanges = () => {
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
      };
      if (removing) {
        // 双端点视觉: 取第一条被删边的两端 (连线通常是单条操作)
        const firstRemovedId = changes.find(
          (ch): ch is EdgeChange & { id: string } => ch.type === 'remove'
        )?.id;
        const victim = get().rfEdges.find((e: Edge) => e.id === firstRemovedId);
        const fromRef = refOf(victim?.source);
        const toRef = refOf(victim?.target);
        withHistoryOp(
          {
            opKey: 'opDeleteEdges',
            detailText: victim ? `${nodeLabelOf(nodeById(victim.source))} → ${nodeLabelOf(nodeById(victim.target))}` : undefined,
            target: { kind: 'edge', ...(fromRef ? { from: fromRef } : {}), ...(toRef ? { to: toRef } : {}) },
          },
          applyChanges,
          { coalesceKey: 'edge.remove' }
        );
      } else {
        applyChanges();
      }
    },

    onConnect: (connection) => {
      const fromRef = refOf(connection.source);
      const toRef = refOf(connection.target);
      const fromLabel = nodeLabelOf(get().rfNodes.find((n: Node) => n.id === connection.source));
      const toLabel = nodeLabelOf(get().rfNodes.find((n: Node) => n.id === connection.target));
      return withHistoryOp(
        {
          opKey: 'opConnectNodes',
          detailText: `${fromLabel} → ${toLabel}`,
          target: {
            kind: 'edge',
            ...(fromRef ? { from: fromRef } : {}),
            ...(toRef ? { to: toRef } : {}),
          },
        },
        () => {
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
        }
      );
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
