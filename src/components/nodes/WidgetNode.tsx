import { memo, useCallback, useEffect, useRef, useState } from 'react';
import { Handle, NodeResizer, Position, useUpdateNodeInternals, type NodeProps } from '@xyflow/react';
import { useAppStore } from '../../store/appStore';
import { useDockStore } from '../../store/dockStore';
import { t } from '../../i18n';
import { X, Settings2 } from 'lucide-react';
import { widgetMinSize } from '../../lib/utils/widgetSize';
import { CanvasErrorTooltip, useCanvasNodeError } from '../ui/CanvasErrorTooltip';
import { getWidgetPorts, deriveRawDataPorts } from './WidgetPorts';
import type { WidgetConfig, DomainType } from '../../types';
import type { Lang } from '../../i18n';
import { getWidgetCategory, WIDGET_CATEGORY_COLORS } from '../../types';
import { rawDataPortId } from '../../lib/utils/nodeDef';
import { widgetToTab } from '../../lib/utils/widgetTab';
import { WIDGET_REGISTRY, widgetComponent } from '../widgets/registry';
import { domainColor } from '../widgets/shared/portVisuals';

/// 端口域标注文案 (悬停提示)
function domainLabel(lang: Lang, domain: DomainType): string {
  return domain === 'freq'
    ? t(lang, 'domainFreq')
    : domain === 'bytes'
      ? t(lang, 'domainBytes')
      : domain === 'string'
        ? t(lang, 'domainString')
        : t(lang, 'domainTime');
}

/// 控件节点 — 统一外壳 (标题栏/端口/缩放/删除) + 注册表分发内容组件。
/// 控件本体是「纯内容」组件 (见 widgets/registryTypes.ts), 不含卡片 chrome。
export const WidgetNode = memo(function WidgetNode({ id, data, selected }: NodeProps) {
  const widget = data.widget as WidgetConfig | undefined;
  const removeWidget = useAppStore((s) => s.removeWidget);
  const updateWidget = useAppStore((s) => s.updateWidget);
  const openCustomEditor = useAppStore((s) => s.openCustomEditor);
  const rfEdges = useAppStore((s) => s.rfEdges);
  const lang = useAppStore((s) => s.lang);
  const nodeTabId = data.tabId as string | undefined;
  const errorMessage = useCanvasNodeError(id, nodeTabId);
  // 持久高亮 — compile-results Tab 点击 source/target 后由 setCanvasHighlight 写入,
  // 与 highlightedNodeId 同步; 错误优先 (error 时不覆盖红框)
  const canvasHighlight = useAppStore((s) => s.canvasHighlight);
  const isCanvasHighlighted =
    !!canvasHighlight && canvasHighlight.nodeId === id && !errorMessage;

  // 稳定回调 — memo 包装的内容组件依赖同引用 props 才能跳过重渲染
  const onRemove = useCallback(() => removeWidget(id), [removeWidget, id]);
  const handleEditCustom = useCallback(() => openCustomEditor(id), [openCustomEditor, id]);
  const updateNodeInternals = useUpdateNodeInternals();
  const widgetLabel = widget?.params.label ?? '';
  const [renaming, setRenaming] = useState(false);
  const [nameDraft, setNameDraft] = useState(widgetLabel);
  const [nameInvalid, setNameInvalid] = useState(false);
  const nameInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!renaming) {
      setNameDraft(widgetLabel);
      setNameInvalid(false);
    }
  }, [renaming, widgetLabel]);

  useEffect(() => {
    if (!renaming) return;
    nameInputRef.current?.focus();
    nameInputRef.current?.select();
  }, [renaming]);

  const commitName = useCallback(() => {
    if (!widget) return;
    const label = nameDraft.trim();
    if (label === '') {
      setNameInvalid(true);
      return;
    }
    if (label !== widget.params.label) {
      updateWidget(id, {
        ...widget,
        params: { ...widget.params, label },
      } as WidgetConfig);
    }
    setRenaming(false);
  }, [id, nameDraft, updateWidget, widget]);

  // 双击节点重新打开数据窗口: 窗口已存在则激活, 已关闭则重新创建
  // (窗口 Tab id 与控件 id 相同 — 与 addWidget 自动建 Tab 共用 widgetToTab 映射)
  const handleOpenWindow = useCallback(() => {
    if (!widget) return;
    const tab = widgetToTab(widget);
    if (!tab) return;
    const st = useAppStore.getState();
    if (st.dataTabs.some((t) => t.id === tab.id)) {
      const dock = useDockStore.getState();
      const card = Object.values(dock.cards).find(
        (c) => c.kind === 'data' && c.tabIds.includes(tab.id)
      );
      if (card) dock.setActiveTab(card.id, tab.id);
      else st.setActiveDataTab(tab.id);
    } else {
      st.addDataTab(tab);
    }
  }, [widget]);

  // 端口 id 集合签名 (与下方渲染用 effectivePorts 同源, 提前算一份供 hook 依赖)
  const widgetPortsKey = widget
    ? (() => {
        const p = widget.kind === 'RawData' ? deriveRawDataPorts(rfEdges, id) : getWidgetPorts(widget);
        return `${p.inputs.map((x) => x.id).join(',')}|${p.outputs.map((x) => x.id).join(',')}`;
      })()
    : '';
  // 端口 id 集合变化 (var_ref 增删 / loopback 开关增减 loopback 端口) 后,
  // 必须通知 React Flow 重测 handle 位置, 否则新端口可见但无法连接
  useEffect(() => {
    updateNodeInternals(id);
  }, [updateNodeInternals, id, widgetPortsKey]);

  if (!widget) {
    return <div className="p-2 text-red text-xs">Missing widget</div>;
  }

  // RawData 输入端口动态派生自连接边 (每个已连接的 source = 一个通道端口), 其余控件用静态定义
  const effectivePorts = widget.kind === 'RawData' ? deriveRawDataPorts(rfEdges, id) : getWidgetPorts(widget);
  // 按控件类别着色 (与 WidgetPalette 分组颜色一致)
  const def = WIDGET_REGISTRY[widget.kind];
  const categoryColor = WIDGET_CATEGORY_COLORS[getWidgetCategory(widget.kind)];
  const editable = def.customEditor != null;
  // 已连接的端口集合 — 用于 Handle 实色填充
  const connectedHandles = new Set<string>();
  for (const e of rfEdges) {
    if (e.source === id && e.sourceHandle) connectedHandles.add(e.sourceHandle);
    if (e.target === id && e.targetHandle) connectedHandles.add(e.targetHandle);
    // RawData 动态端口 id 是 `src:<sourceId>:<handle>` — 按 (source, sourceHandle) 标记已连接
    if (widget.kind === 'RawData' && e.target === id) connectedHandles.add(rawDataPortId(e.source, e.sourceHandle));
  }

  // 注册表分发内容组件 — 所有 kind 一行查找 (含窗口型控件的占位组件)
  const Content = widgetComponent(widget.kind);

  return (
    <CanvasErrorTooltip message={errorMessage}>
      <div
        className="nowheel widget-card-acrylic rounded-md w-full h-full min-w-[140px] min-h-[48px] text-[11px] relative flex flex-col [&.selected]:border-accent"
        style={
          errorMessage
            ? { boxShadow: '0 0 0 2px #ef4444' }
            : isCanvasHighlighted
              ? { boxShadow: '0 0 0 2px var(--color-accent)' }
              : undefined
        }
        onDoubleClick={widgetToTab(widget) ? handleOpenWindow : undefined}
        title={widgetToTab(widget) ? t(lang, 'nodeOpenWindowHint') : undefined}
      >
      {/* 拖拽调整大小 — 选中时显示 8 向手柄; 双击手柄不得冒泡成「打开数据窗口」 */}
      <div className="contents" onDoubleClick={(e) => e.stopPropagation()}>
        <NodeResizer
          isVisible={!!selected}
          minWidth={widgetMinSize(widget.kind).minW}
          minHeight={widgetMinSize(widget.kind).minH}
          lineClassName="!border-accent"
          handleClassName="!w-[7px] !h-[7px] !border-accent !bg-bg-sidebar"
        />
      </div>
      <div
        className="node-drag-handle flex items-center justify-between px-1.5 py-1 border-b border-border text-[10px] font-semibold uppercase tracking-[0.4px] cursor-grab active:cursor-grabbing"
        style={{ color: categoryColor }}
        onDoubleClick={(event) => {
          event.preventDefault();
          event.stopPropagation();
          setNameDraft(widgetLabel);
          setNameInvalid(false);
          setRenaming(true);
        }}
        title={t(lang, 'nodeRenameHint')}
      >
        {renaming ? (
          <input
            ref={nameInputRef}
            className={`nodrag nowheel flex-1 min-w-0 h-5 px-1 rounded bg-bg-input text-text-primary normal-case tracking-normal outline-none border ${nameInvalid ? 'border-red' : 'border-accent'}`}
            value={nameDraft}
            onChange={(event) => { setNameDraft(event.target.value); setNameInvalid(false); }}
            onPointerDown={(event) => event.stopPropagation()}
            onClick={(event) => event.stopPropagation()}
            onDoubleClick={(event) => event.stopPropagation()}
            onBlur={commitName}
            onKeyDown={(event) => {
              event.stopPropagation();
              if (event.key === 'Enter') {
                event.preventDefault();
                commitName();
              } else if (event.key === 'Escape') {
                event.preventDefault();
                setNameDraft(widgetLabel);
                setNameInvalid(false);
                setRenaming(false);
              }
            }}
            aria-label={t(lang, 'widgetName')}
            aria-invalid={nameInvalid}
          />
        ) : (
          <span className="flex-1 truncate" title={widgetLabel || widget.kind}>
            {widgetLabel || widget.kind}
          </span>
        )}
        {editable && (
          <button
            className="nodrag w-4 h-4 p-0 opacity-60 hover:opacity-100 flex items-center justify-center rounded hover:bg-bg-hover transition-opacity"
            onClick={(e) => {
              e.stopPropagation();
              handleEditCustom();
            }}
            title="Edit"
          >
            <Settings2 size={10} />
          </button>
        )}
        <button
          className="nodrag w-4 h-4 p-0 opacity-60 hover:opacity-100 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover transition-opacity"
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
        >
          <X size={10} />
        </button>
      </div>
      <div className="flex flex-row w-full flex-1 min-h-[32px]">
        {/* 输入端口 (左侧) — 融入普通文档流 */}
        <div className="flex flex-col justify-center gap-0.5 py-1 -ml-1.5 z-10">
          {effectivePorts.inputs.map((port) => (
            <div
              key={port.id}
              className="flex items-center gap-1 h-[14px] relative"
              title={`${port.label} · ${domainLabel(lang, port.domain)}`}
            >
              <Handle
                type="target"
                position={Position.Left}
                id={port.id}
                style={{
                  position: 'relative',
                  left: 'auto',
                  top: 'auto',
                  transform: 'none',
                  borderColor: domainColor(port.domain),
                }}
                className={`w-[9px] h-[9px] bg-bg-input border-[1.5px] rounded-full cursor-crosshair transition-all duration-150 hover:bg-accent hover:scale-130 [&.connectingto]:bg-green [&.connectingto]:border-green [&.valid]:bg-green [&.valid]:border-green${connectedHandles.has(port.id) ? ' connected' : ''}`}
              />
              <span className="text-[9px] text-text-secondary font-mono whitespace-nowrap bg-bg-sidebar px-0.5 py-px rounded-sm">{port.label}</span>
              <span
                className="w-[5px] h-[5px] rounded-full flex-shrink-0 pointer-events-none"
                style={{ backgroundColor: domainColor(port.domain) }}
              />
            </div>
          ))}
        </div>

        {/* 主内容区 — 注册表分发的纯内容组件 */}
        <div className="flex-1 p-2 flex flex-col justify-center min-w-0">
          <div className="flex flex-col gap-1.5">
            <Content widget={widget} />
          </div>
        </div>

        {/* 输出端口 (右侧) — 融入普通文档流 */}
        <div className="flex flex-col items-end justify-center gap-0.5 py-1 -mr-1.5 z-10">
          {effectivePorts.outputs.map((port) => (
            <div
              key={port.id}
              className="flex items-center justify-end gap-1 h-[14px] relative"
              title={`${port.label} · ${domainLabel(lang, port.domain)}`}
            >
              <span
                className="w-[5px] h-[5px] rounded-full flex-shrink-0 pointer-events-none"
                style={{ backgroundColor: domainColor(port.domain) }}
              />
              <span className="text-[9px] text-text-secondary font-mono whitespace-nowrap bg-bg-sidebar px-0.5 py-px rounded-sm">{port.label}</span>
              <Handle
                type="source"
                position={Position.Right}
                id={port.id}
                style={{
                  position: 'relative',
                  right: 'auto',
                  top: 'auto',
                  transform: 'none',
                  borderColor: domainColor(port.domain),
                }}
                className={`w-[9px] h-[9px] bg-bg-input border-[1.5px] rounded-full cursor-crosshair transition-all duration-150 hover:bg-accent hover:scale-130 [&.connectingto]:bg-green [&.connectingto]:border-green [&.valid]:bg-green [&.valid]:border-green${connectedHandles.has(port.id) ? ' connected' : ''}`}
              />
            </div>
          ))}
        </div>
      </div>
    </div>
    </CanvasErrorTooltip>
  );
});
