import { memo, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  ReactFlow,
  ReactFlowProvider,
  Background,
  Controls,
  MiniMap,
  useReactFlow,
  type NodeTypes,
  type Edge,
  type Node,
  Panel,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import { useAppStore } from '../../store/appStore';
import { createWidget } from '../../lib/utils/createWidget';
import { t } from '../../i18n';
import { useContextMenu } from '../../lib/hooks/useContextMenu';
import { transitionStore } from '../../lib/utils/transitionStore';
import { dockDrag, type WidgetDragSpec } from '../../lib/dockDrag';
import type { WidgetConfig, MathOp, FilterPresetKind } from '../../types';
import { UNARY_MATH_OPS } from '../../types';
import { ChannelSourceNode } from '../nodes/ChannelSourceNode';
import { WidgetNode } from '../nodes/WidgetNode';
import { Maximize, LayoutGrid } from 'lucide-react';

interface NodeEditorProps {
  tabId: string;
}

/// 节点类型注册 — React Flow 要求在组件外部定义以避免无限渲染
const nodeTypes: NodeTypes = {
  channelSource: ChannelSourceNode,
  widget: WidgetNode,
};

/// React Flow 风格节点编辑器
/// - 从侧边栏拖拽控件到画布 (onDrop 绑在 <ReactFlow> 上, 外层 div 不拦截)
/// - 节点之间通过边连接表示数据流
/// - 通道源节点自动存在, 输出 ch0..chN
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
  const setSidebarView = useAppStore((s) => s.setSidebarView);
  const reactFlow = useReactFlow();
  // 控件拖拽悬停画布时的高亮 (dockDrag 控制器驱动)
  const [isDragOver, setIsDragOver] = useState(false);
  useEffect(() => dockDrag.subscribeCanvasHover(setIsDragOver), []);

  const onCanvasContextMenu = useContextMenu([
    {
      id: 'fit-view',
      label: t(lang, 'fitView'),
      icon: <Maximize />,
      onClick: () => reactFlow.fitView({ padding: 0.2 }),
    },
    {
      id: 'reset-zoom',
      label: t(lang, 'resetZoom'),
      icon: <Maximize />,
      onClick: () => reactFlow.zoomTo(1),
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

  // 按当前 tab 过滤节点和边
  const tabNodes = useMemo(
    () =>
      rfNodes.filter(
        (n) => n.data.tabId === tabId || (n.type === 'channelSource' && n.id.endsWith(`-${tabId}`))
      ) as Node[],
    [rfNodes, tabId]
  );

  const tabNodeIds = useMemo(
    () => new Set(tabNodes.map((n) => n.id)),
    [tabNodes]
  );

  const tabEdges = useMemo(
    () => rfEdges.filter((e) => tabNodeIds.has(e.source) && tabNodeIds.has(e.target)) as Edge[],
    [rfEdges, tabNodeIds]
  );

  // 从控件面板拖出控件 — 指针事件落点 (dockDrag 控制器命中 canvas 投放区后调用)
  const createFromDrop = useCallback(
    (x: number, y: number, spec: WidgetDragSpec) => {
      // 用 screenToFlowPosition 正确处理 zoom/pan 后的坐标转换
      const position = reactFlow.screenToFlowPosition({ x, y });

      const widget = createWidget(spec.kind);
      // 算术控件: 应用拖拽时携带的 op
      if (widget.kind === 'Math' && spec.op) {
        const mathWidget = widget as Extract<WidgetConfig, { kind: 'Math' }>;
        mathWidget.params.op = spec.op as MathOp;
        if (UNARY_MATH_OPS.includes(spec.op as MathOp)) {
          mathWidget.params.inputCount = 1;
        }
        mathWidget.params.label = `Math ${spec.op}`;
      }
      // 滤波器控件: 应用拖拽时携带的 preset
      if (widget.kind === 'Filter' && spec.preset) {
        const filterWidget = widget as Extract<WidgetConfig, { kind: 'Filter' }>;
        filterWidget.params.preset = spec.preset as FilterPresetKind;
        filterWidget.params.label = `Filter ${spec.preset}`;
      }
      addWidget(widget, tabId, position);
    },
    [addWidget, tabId, reactFlow]
  );

  // 当前可见画布注册为控件投放目标 (按画布元素注册 — 多个控制卡片并存时落点各归其主)
  useEffect(() => {
    const el = wrapperRef.current;
    if (!el) return;
    dockDrag.registerCanvasHandler(el, (x, y, spec) => createFromDrop(x, y, spec));
    return () => dockDrag.registerCanvasHandler(el, null);
  }, [createFromDrop]);

  // 回环节点连线校验: loopbackOut (字节发送口) 只能连 loopbackIn (字节输入口), 反之亦然;
  // 普通数值口之间维持现状 (不做类型限制)
  const isValidConnection = useCallback(
    (conn: { sourceHandle?: string | null; targetHandle?: string | null }) => {
      const fromLoopback = conn.sourceHandle === 'loopbackOut';
      const toLoopback = conn.targetHandle === 'loopbackIn';
      // 一端是回环口时, 另一端必须也是对应回环口; 两端都不是则放行
      return fromLoopback === toLoopback;
    },
    []
  );

  return (
    <div
      className={`absolute inset-0 bg-bg-editor overflow-hidden node-editor-rf${isDragOver ? ' drag-over' : ''}`}
      ref={wrapperRef}
      onContextMenu={onCanvasContextMenu}
      data-dock-zone="canvas"
    >
      <ReactFlow
        nodes={tabNodes}
        edges={tabEdges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={onConnect}
        isValidConnection={isValidConnection}
        nodeTypes={nodeTypes}
        fitView
        fitViewOptions={{ padding: 0.2 }}
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
          nodeColor={(n) => (n.type === 'channelSource' ? '#75beff' : '#89d185')}
        />
        <Panel position="top-left">
          {tabNodes.length <= 1 && (
            <div className="bg-bg-panel-header border border-dashed border-border rounded px-2.5 py-1.5 text-xs text-text-secondary pointer-events-none">{t(lang, 'dragWidgetHint')}</div>
          )}
        </Panel>
      </ReactFlow>
    </div>
  );
}
