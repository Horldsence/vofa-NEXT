import { memo, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  ReactFlow,
  ReactFlowProvider,
  Background,
  Controls,
  MiniMap,
  useReactFlow,
  type NodeTypes,
  Panel,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import { useAppStore } from '../../store/appStore';
import { createWidget } from '../../lib/utils/createWidget';
import { t } from '../../i18n';
import { notify } from '../../lib/tauri/notifications';
import { useContextMenu } from '../../lib/hooks/useContextMenu';
import { transitionStore } from '../../lib/utils/transitionStore';
import { dockDrag, type WidgetDragSpec } from '../../lib/dockDrag';
import type { MathOp, StrOp } from '../../types';
import { isUnaryMathOp } from '../../types';
import { WidgetNode } from '../nodes/WidgetNode';
import { TransportNode } from '../nodes/TransportNode';
import { ProtocolNode } from '../nodes/ProtocolNode';
import { GlobalNodeProperties } from '../nodes/GlobalNodeProperties';
import { WidgetProperties } from '../nodes/WidgetProperties';
import { validateConnection } from '../../lib/utils/connectionRules';
import { Maximize, LayoutGrid } from 'lucide-react';

interface NodeEditorProps {
  tabId: string;
}

/// 节点类型注册 — React Flow 要求在组件外部定义以避免无限渲染
const nodeTypes: NodeTypes = {
  transport: TransportNode,
  protocol: ProtocolNode,
  widget: WidgetNode,
};

/// React Flow 风格节点编辑器
/// - 从侧边栏拖拽控件到画布 (onDrop 绑在 <ReactFlow> 上, 外层 div 不拦截)
/// - 节点之间通过边连接表示数据流
/// - 全局节点 (Transport/Protocol) 渲染在所有 tab 画布上
///
/// 必须用 ReactFlowProvider 包裹, 才能在内部使用 useReactFlow().screenToFlowPosition()
/// 否则拖拽放置的节点会落到错误的画布坐标 (尤其 fitView/pan/zoom 后)
export const NodeEditor = memo(function NodeEditor({ tabId }: NodeEditorProps) {
  return (
    <ReactFlowProvider>
      <NodeEditorInner tabId={tabId} />
    </ReactFlowProvider>
  );
});

function NodeEditorInner({ tabId }: NodeEditorProps) {
  const lang = useAppStore((s) => s.lang);
  const rfNodes = useAppStore((s) => s.rfNodes);
  const rfEdges = useAppStore((s) => s.rfEdges);
  const onNodesChange = useAppStore((s) => s.onNodesChange);
  const onEdgesChange = useAppStore((s) => s.onEdgesChange);
  const onConnect = useAppStore((s) => s.onConnect);
  const addWidget = useAppStore((s) => s.addWidget);
  const addTransportNode = useAppStore((s) => s.addTransportNode);
  const addProtocolNode = useAppStore((s) => s.addProtocolNode);
  const setSidebarView = useAppStore((s) => s.setSidebarView);
  const reactFlow = useReactFlow();

  const flyToRequest = useAppStore((s) => s.flyToRequest);
  useEffect(() => {
    if (!flyToRequest) return;
    if (flyToRequest.tabId !== tabId) return;
    const node = rfNodes.find((n) => n.id === flyToRequest.nodeId);
    if (node) {
      void reactFlow.setCenter(node.position.x, node.position.y, {
        duration: 400,
        zoom: 1.5,
      });
    }
    useAppStore.getState().clearFlyToRequest();
  }, [flyToRequest, tabId, rfNodes, reactFlow]);

  // 控件拖拽悬停画布时的高亮 (dockDrag 控制器驱动)
  const [isDragOver, setIsDragOver] = useState(false);
  useEffect(() => dockDrag.subscribeCanvasHover(setIsDragOver), []);

  const onCanvasContextMenu = useContextMenu([
    {
      id: 'fit-view',
      label: t(lang, 'fitView'),
      icon: <Maximize />,
      onClick: () => { void reactFlow.fitView({ padding: 0.2 }); },
    },
    {
      id: 'reset-zoom',
      label: t(lang, 'resetZoom'),
      icon: <Maximize />,
      onClick: () => { void reactFlow.zoomTo(1); },
    },
    { kind: 'separator' },
    {
      id: 'open-widget-palette',
      label: t(lang, 'widgetPalette'),
      icon: <LayoutGrid />,
      onClick: () => transitionStore(() => setSidebarView('widgets')),
    },
  ]);

  // React Flow 容器引用 — 仅用于视觉反馈 (drag-over 高亮)
  const wrapperRef = useRef<HTMLDivElement>(null);

  // 按当前 tab 过滤节点: 本 tab 的 widget 节点 + 全部全局节点 (Transport/Protocol)
  const tabNodes = useMemo(
    () =>
      rfNodes
        .filter((n) => n.data.tabId === tabId || n.data.global === true)
        .map((n) => ({ ...n, dragHandle: '.node-drag-handle' })),
    [rfNodes, tabId]
  );

  const tabNodeIds = useMemo(
    () => new Set(tabNodes.map((n) => n.id)),
    [tabNodes]
  );

  const tabState = useAppStore((s) => s.tabStates[tabId]);
  const EMPTY_EDGES: readonly string[] = [];
  const tabErrorEdges = useAppStore((s) => s.tabErrorEdges[tabId] ?? EMPTY_EDGES);

  const tabEdges = useMemo(() => {
    const erroredSet = tabState === 'error' ? new Set(tabErrorEdges) : new Set<string>();
    return rfEdges
      .filter((e) => tabNodeIds.has(e.source) && tabNodeIds.has(e.target))
      .map((e) => {
        if (!erroredSet.has(e.id)) return e;
        return {
          ...e,
          style: { ...(e.style ?? {}), stroke: '#ef4444', strokeWidth: 2 },
          animated: true,
          className: 'compile-error-edge',
        };
      });
  }, [rfEdges, tabNodeIds, tabErrorEdges, tabState]);

  // 选中的全局节点 → 右侧属性面板
  const selectedGlobalNode = useMemo(
    () => tabNodes.find((n) => n.selected && n.data.global === true),
    [tabNodes]
  );

  const selectedWidgetNode = useMemo(
    () => tabNodes.find((n) => n.selected && n.type === 'widget'),
    [tabNodes]
  );

  // 从控件面板拖出控件 — 指针事件落点 (dockDrag 控制器命中 canvas 投放区后调用)
  const createFromDrop = useCallback(
    (x: number, y: number, spec: WidgetDragSpec) => {
      // 用 screenToFlowPosition 正确处理 zoom/pan 后的坐标转换
      const position = reactFlow.screenToFlowPosition({ x, y });

      // 全局节点 (数据接口 / 协议引擎)
      if (spec.globalNode === 'transport') {
        addTransportNode(spec.transportKind ?? 'Serial', position);
        return;
      }
      if (spec.globalNode === 'protocol') {
        addProtocolNode(undefined, position);
        return;
      }
      if (!spec.kind) return;

      const widget = createWidget(spec.kind);
      // 算术控件: 应用拖拽时携带的 op
      if (widget.kind === 'Math' && spec.op) {
        const mathWidget = widget;
        mathWidget.params.op = spec.op as MathOp;
        if (isUnaryMathOp(spec.op as MathOp)) {
          mathWidget.params.inputCount = 1;
        }
        mathWidget.params.label = `Math ${spec.op}`;
      }
      // 字符串控件: 应用拖拽时携带的 op
      if (widget.kind === 'Str' && spec.op) {
        const strWidget = widget;
        strWidget.params.op = spec.op as StrOp;
        strWidget.params.label = `Str ${spec.op}`;
      }
      // 滤波器控件: 应用拖拽时携带的 preset
      if (widget.kind === 'Filter' && spec.preset) {
        const filterWidget = widget;
        filterWidget.params.preset = spec.preset;
        filterWidget.params.label = `Filter ${spec.preset}`;
      }
      addWidget(widget, tabId, position);
    },
    [addWidget, addTransportNode, addProtocolNode, tabId, reactFlow]
  );

  // 当前可见画布注册为控件投放目标 (按画布元素注册 — 多个控制卡片并存时落点各归其主)
  useEffect(() => {
    const el = wrapperRef.current;
    if (!el) return;
    dockDrag.registerCanvasHandler(el, (x, y, spec) => createFromDrop(x, y, spec));
    return () => dockDrag.registerCanvasHandler(el, null);
  }, [createFromDrop]);

  // 连线校验 — 统一规则单一权威 (lib/utils/connectionRules): 节点存在 / 同 tab /
  // 端口存在 / 域匹配 (time/freq/bytes/string 同域; RawData 是 bytes/time 双域 Sink
  // 仅拒 freq)。与后端编译校验同域规则, 手动拖拽在此前置拦截并提示。
  const isValidConnection = useCallback(
    (conn: {
      source?: string | null;
      target?: string | null;
      sourceHandle?: string | null;
      targetHandle?: string | null;
    }) => {
      if (!conn.source || !conn.target) return false;
      const state = useAppStore.getState();
      const check = validateConnection(
        {
          nodes: state.rfNodes,
          derivedPorts: state.derivedPorts,
          detectedChannels: state.detectedChannels,
        },
        {
          source: conn.source,
          target: conn.target,
          sourceHandle: conn.sourceHandle,
          targetHandle: conn.targetHandle,
        }
      );
      if (!check.ok) {
        notify.warn(t(lang, 'connectionRejectedTitle'), check.message ?? t(lang, 'domainMismatchMsg'), {
          source: 'domain-mismatch',
        });
        return false;
      }
      return true;
    },
    [lang]
  );

  // 连线有效性不做前端自愈 — 后端编译是连线唯一权威, graph:source 回声即真值;
  // 渲染层瞬时错误 (#008) 只说明端口重测尚未完成, 据此删边会误删后端认可的连线

  return (
    <div
      className={`absolute inset-0 bg-bg-editor overflow-hidden node-editor-rf${isDragOver ? ' drag-over' : ''}`}
      ref={wrapperRef}
      onContextMenu={onCanvasContextMenu}
      data-dock-zone="canvas"
      data-tour="canvas"
    >
      <ReactFlow
        nodes={tabNodes}
        edges={tabEdges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={onConnect}
        isValidConnection={isValidConnection}
        nodeTypes={nodeTypes}
        defaultEdgeOptions={{ interactionWidth: 40, style: { strokeWidth: 2 } }}
        fitView
        fitViewOptions={{ padding: 0.2, minZoom: 1, maxZoom: 1 }}
        minZoom={0.2}
        maxZoom={2.5}
        proOptions={{ hideAttribution: true }}
      >
        <Background gap={12} size={1} />
        <Controls position="bottom-right" showInteractive={false} />
        <MiniMap
          pannable
          zoomable
          className="bg-bg-sidebar border border-border rounded overflow-hidden"
          nodeColor={(n) =>
            n.type === 'transport' ? '#e5c07b' : n.type === 'protocol' ? '#75beff' : '#89d185'
          }
        />
        <Panel position="top-left">
          {!tabNodes.some((n) => n.data.tabId === tabId) && (
            <div className="bg-bg-panel-header border border-dashed border-border rounded px-2.5 py-1.5 text-xs text-text-secondary pointer-events-none">{t(lang, 'dragWidgetHint')}</div>
          )}
        </Panel>
      </ReactFlow>
      {selectedGlobalNode
        ? <GlobalNodeProperties node={selectedGlobalNode} />
        : selectedWidgetNode && <WidgetProperties node={selectedWidgetNode} />}
    </div>
  );
}
