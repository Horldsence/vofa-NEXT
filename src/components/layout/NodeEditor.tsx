import { useCallback, useMemo, useRef } from 'react';
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
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
export function NodeEditor({ tabId }: NodeEditorProps) {
  const lang = useAppStore((s) => s.lang);
  const rfNodes = useAppStore((s) => s.rfNodes);
  const rfEdges = useAppStore((s) => s.rfEdges);
  const onNodesChange = useAppStore((s) => s.onNodesChange);
  const onEdgesChange = useAppStore((s) => s.onEdgesChange);
  const onConnect = useAppStore((s) => s.onConnect);
  const addWidget = useAppStore((s) => s.addWidget);

  // React Flow 容器引用 — 用于坐标转换 (无需 useReactFlow hook)
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
  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      const kind = e.dataTransfer.getData('application/widget-kind') as WidgetConfig['kind'] | '';
      if (!kind) return;

      // 用容器 bounding rect 把屏幕坐标转为画布相对坐标 (不含 zoom/pan, 但 fitView 后足够准)
      const wrapper = wrapperRef.current;
      const rect = wrapper?.getBoundingClientRect();
      const position = rect
        ? { x: e.clientX - rect.left, y: e.clientY - rect.top }
        : { x: 240, y: 80 };

      const widget = createWidget(kind);
      addWidget(widget, tabId, position);
    },
    [addWidget, tabId]
  );

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'copy';
  }, []);

  return (
    <div className="node-editor-rf" ref={wrapperRef}>
      <ReactFlow
        nodes={tabNodes}
        edges={tabEdges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={onConnect}
        onDrop={handleDrop}
        onDragOver={handleDragOver}
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
