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
import { notify } from '../../lib/tauri/notifications';
import { useContextMenu } from '../../lib/hooks/useContextMenu';
import { transitionStore } from '../../lib/utils/transitionStore';
import { dockDrag, type WidgetDragSpec } from '../../lib/dockDrag';
import type { WidgetConfig, MathOp, StrOp, FilterPresetKind, DomainType } from '../../types';
import { isUnaryMathOp } from '../../types';
import { WidgetNode, getWidgetPorts } from '../nodes/WidgetNode';
import { TransportNode } from '../nodes/TransportNode';
import { ProtocolNode } from '../nodes/ProtocolNode';
import { GlobalNodeProperties } from '../nodes/GlobalNodeProperties';
import { isRawDataPreset } from '../../lib/utils/protocolSchema';
import type { ProtocolNodeData } from '../../store/appStoreHelpers';
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

  // 按当前 tab 过滤节点: 本 tab 的 widget 节点 + 全部全局节点 (Transport/Protocol)
  const tabNodes = useMemo(
    () =>
      rfNodes.filter((n) => n.data.tabId === tabId || n.data.global === true) as Node[],
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

  // 选中的全局节点 → 右侧属性面板
  const selectedGlobalNode = useMemo(
    () => tabNodes.find((n) => n.selected && n.data.global === true),
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
        const mathWidget = widget as Extract<WidgetConfig, { kind: 'Math' }>;
        mathWidget.params.op = spec.op as MathOp;
        if (isUnaryMathOp(spec.op as MathOp)) {
          mathWidget.params.inputCount = 1;
        }
        mathWidget.params.label = `Math ${spec.op}`;
      }
      // 字符串控件: 应用拖拽时携带的 op
      if (widget.kind === 'Str' && spec.op) {
        const strWidget = widget as Extract<WidgetConfig, { kind: 'Str' }>;
        strWidget.params.op = spec.op as StrOp;
        strWidget.params.label = `Str ${spec.op}`;
      }
      // 滤波器控件: 应用拖拽时携带的 preset
      if (widget.kind === 'Filter' && spec.preset) {
        const filterWidget = widget as Extract<WidgetConfig, { kind: 'Filter' }>;
        filterWidget.params.preset = spec.preset as FilterPresetKind;
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

  // 端口域解析: 全局节点按端口表 (rx/tx/in/out=字节, chN=时域, str=字符串); 控件端口按 getWidgetPorts 的 domain 标注
  const resolveDomain = useCallback(
    (nodeId: string | null, handleId: string | null | undefined, kind: 'source' | 'target'): DomainType | null => {
      if (!nodeId || !handleId) return null;
      const node = useAppStore.getState().rfNodes.find((n: Node) => n.id === nodeId);
      if (!node) return null;
      if (node.type === 'transport') {
        if (kind === 'source' && handleId === 'rx') return 'bytes';
        if (kind === 'target' && handleId === 'tx') return 'bytes';
        return null;
      }
      if (node.type === 'protocol') {
        if (handleId === 'in' || handleId === 'out') return 'bytes';
        // RawData 预设的 str 口是字符串域 (UTF-8 lossy 文本); custom schema 的同名端口除外
        if (kind === 'source' && handleId === 'str'
          && isRawDataPreset(node.data as unknown as ProtocolNodeData)) return 'string';
        if (kind === 'source' && /^ch\d+$/.test(handleId)) return 'time';
        return null;
      }
      const widget = node.data?.widget as WidgetConfig | undefined;
      if (!widget) return null;
      // RawData 输入端口是动态派生的 (src:<source>:<handle>), 静态端口表查不到 — 一律按时域。
      // 注: RawData 实际同时接受字节域/时域源 (见 isValidConnection 的 RawData 特判),
      // 此处返回 time 只是让频域输出在通用校验下仍被拦截
      if (widget.kind === 'RawData') return 'time';
      // FrameDecoder 旧版回环字节输入口 (兼容旧图数据)
      if (widget.kind === 'FrameDecoder' && kind === 'target' && handleId === 'loopbackIn') return 'bytes';
      const ports = getWidgetPorts(widget);
      const list = kind === 'source' ? ports.outputs : ports.inputs;
      return list.find((p) => p.id === handleId)?.domain ?? null;
    },
    []
  );

  // 连线校验: 时域/频域/字节域端口必须同域, 跨域阻止并提示
  // (字节口: Transport rx/tx, Protocol in/out, FrameDecoder in, Command loopbackOut)
  // 例外: RawData 是字节/时域双域 Sink — Transport rx / Protocol out 等字节源可直连
  // (通道显示该接口原始字节流), 仅频域源仍阻止
  const isValidConnection = useCallback(
    (conn: {
      source?: string | null;
      target?: string | null;
      sourceHandle?: string | null;
      targetHandle?: string | null;
    }) => {
      const sd = resolveDomain(conn.source ?? null, conn.sourceHandle, 'source');
      const targetNode = useAppStore.getState().rfNodes.find((n: Node) => n.id === conn.target);
      const targetWidget = targetNode?.data?.widget as WidgetConfig | undefined;
      if (targetWidget?.kind === 'RawData') {
        if (sd === 'freq') {
          notify.warn(t(lang, 'domainMismatchTitle'), t(lang, 'domainMismatchMsg'), {
            source: 'domain-mismatch',
          });
          return false;
        }
        return true;
      }
      const td = resolveDomain(conn.target ?? null, conn.targetHandle, 'target');
      if (sd && td && sd !== td) {
        notify.warn(t(lang, 'domainMismatchTitle'), t(lang, 'domainMismatchMsg'), {
          source: 'domain-mismatch',
        });
        return false;
      }
      return true;
    },
    [resolveDomain, lang]
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
      {selectedGlobalNode && <GlobalNodeProperties node={selectedGlobalNode} />}
    </div>
  );
}
