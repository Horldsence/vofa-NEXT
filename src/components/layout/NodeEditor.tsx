import { useCallback, useMemo, useRef, useState } from 'react';
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
import { useAppStore, createWidget } from '../../store/appStore';
import { t } from '../../i18n';
import type { WidgetConfig } from '../../types';
import { ChannelSourceNode } from '../nodes/ChannelSourceNode';
import { WidgetNode } from '../nodes/WidgetNode';

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
export function NodeEditor({ tabId }: NodeEditorProps) {
  return (
    <ReactFlowProvider>
      <NodeEditorInner tabId={tabId} />
    </ReactFlowProvider>
  );
}

function NodeEditorInner({ tabId }: NodeEditorProps) {
  const lang = useAppStore((s) => s.lang);
  const rfNodes = useAppStore((s) => s.rfNodes);
  const rfEdges = useAppStore((s) => s.rfEdges);
  const onNodesChange = useAppStore((s) => s.onNodesChange);
  const onEdgesChange = useAppStore((s) => s.onEdgesChange);
  const onConnect = useAppStore((s) => s.onConnect);
  const addWidget = useAppStore((s) => s.addWidget);
  const reactFlow = useReactFlow();
  const [isDragOver, setIsDragOver] = useState(false);

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

  // 从侧边栏拖拽接收 — 必须绑在 <ReactFlow> 上, 否则被内部 pan/zoom 拦截
  // 关键: 用 screenToFlowPosition 把屏幕坐标转为画布坐标 (考虑 zoom/pan),
  // 否则 fitView 后新节点会落到屏幕外
  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      setIsDragOver(false);
      const kind = e.dataTransfer.getData('application/widget-kind') as WidgetConfig['kind'] | '';
      if (!kind) return;

      // 用 screenToFlowPosition 正确处理 zoom/pan 后的坐标转换
      const position = reactFlow.screenToFlowPosition({
        x: e.clientX,
        y: e.clientY,
      });

      const widget = createWidget(kind);
      addWidget(widget, tabId, position);
    },
    [addWidget, tabId, reactFlow]
  );

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'copy';
    setIsDragOver(true);
  }, []);

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    // 仅当离开整个容器 (relatedTarget 不在内部) 时才清除高亮
    const related = e.relatedTarget as globalThis.Node | null;
    if (!related || !e.currentTarget.contains(related)) {
      setIsDragOver(false);
    }
  }, []);

  return (
    <div
      className={`node-editor-rf${isDragOver ? ' drag-over' : ''}`}
      ref={wrapperRef}
    >
      <ReactFlow
        nodes={tabNodes}
        edges={tabEdges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={onConnect}
        onDrop={handleDrop}
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
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
          className="rf-minimap"
          nodeColor={(n) => (n.type === 'channelSource' ? '#75beff' : '#89d185')}
        />
        <Panel position="top-left">
          {tabNodes.length <= 1 && (
            <div className="rf-empty-hint">{t(lang, 'dragWidgetHint')}</div>
          )}
        </Panel>
      </ReactFlow>
    </div>
  );
}
