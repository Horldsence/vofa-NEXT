import { memo, useCallback, useEffect } from 'react';
import { Handle, Position, useUpdateNodeInternals, type NodeProps, type Edge } from '@xyflow/react';
import { useAppStore } from '../../store/appStore';
import { useDockStore } from '../../store/dockStore';
import { t } from '../../i18n';
import { X, Settings2 } from 'lucide-react';
import { WidgetEmbeddedContext } from '../ui/WidgetCard';
import { CanvasErrorTooltip, useCanvasNodeError } from '../ui/CanvasErrorTooltip';
import { getWidgetPorts, type WidgetPort } from './WidgetPorts';
import type { WidgetConfig, DomainType } from '../../types';
import type { Lang } from '../../i18n';
import { getWidgetCategory, WIDGET_CATEGORY_COLORS } from '../../types';
import { rawDataPortId } from '../../lib/utils/nodeDef';
import { widgetToTab } from '../../lib/utils/widgetTab';
import { Knob } from '../controls/Knob';
import { ButtonWidget } from '../controls/ButtonWidget';
import { Radio } from '../controls/Radio';
import { Checkbox } from '../controls/Checkbox';
import { Slider } from '../controls/Slider';
import { Label } from '../controls/Label';
import { PieChart } from '../displays/widgets/PieChart';
import { ImageViewer } from '../displays/widgets/ImageViewer';
import { Gauge } from '../displays/widgets/Gauge';
import { LED } from '../displays/widgets/LED';
import { NumberDisplay } from '../displays/widgets/NumberDisplay';
import { CustomWidget } from '../displays/widgets/CustomWidget';
import { MathWidget } from '../displays/widgets/MathWidget';
import { FilterWidget } from '../displays/widgets/FilterWidget';
import { FFTWidget } from '../displays/widgets/FFTWidget';
import { IFFTWidget } from '../displays/widgets/IFFTWidget';
import { TextDisplay } from '../displays/widgets/TextDisplay';
import { TextInput } from '../controls/TextInput';
import { StrWidget } from '../displays/widgets/StrWidget';

/// 端口 id 用 `src:<sourceId>:<sourceHandle>` (稳定, 不随源节点 label 变化),
/// label 取源节点的输出端口名 (handle)。尚未连接任何边时回退到单个默认端口, 便于用户建立第一条连接。
/// 域标注: 字节源端口 (Transport rx / Protocol out) 标 bytes (黄色), 其余标 time
function deriveRawDataPorts(
  edges: Edge[],
  nodeId: string
): { inputs: WidgetPort[]; outputs: WidgetPort[] } {
  const seen = new Set<string>();
  const inputs: WidgetPort[] = [];
  for (const e of edges) {
    // 目标是本节点即视为通道连接; 同一 (source, sourceHandle) 去重为一个端口
    if (e.target !== nodeId) continue;
    const handle = e.sourceHandle ?? 'data';
    const key = rawDataPortId(e.source, e.sourceHandle);
    if (seen.has(key)) continue;
    seen.add(key);
    const domain: DomainType = handle === 'rx' || handle === 'out' ? 'bytes' : 'time';
    inputs.push({ id: key, label: handle, domain });
  }
  if (inputs.length === 0) {
    return { inputs: [{ id: 'data', label: 'data', domain: 'time' }], outputs: [] };
  }
  return { inputs, outputs: [] };
}

/// 端口域颜色 — 频域紫色, 时域蓝色, 字节域黄色, 字符串域橙色 (仅作圆点/手柄描边, 不占文字宽度, 避免遮挡)
function domainColor(domain: DomainType): string {
  return domain === 'freq' ? '#ba68c8' : domain === 'bytes' ? '#e5c07b' : domain === 'string' ? '#ffa726' : '#75beff';
}

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

/// 控件节点 — 包装实际控件, 添加 React Flow Handle
export const WidgetNode = memo(function WidgetNode({ id, data }: NodeProps) {
  const widget = data.widget as WidgetConfig | undefined;
  const removeWidget = useAppStore((s) => s.removeWidget);
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

  // 稳定回调 — memo 包装的嵌入控件 (Gauge/LED/...) 依赖同引用 props 才能跳过重渲染
  const onRemove = useCallback(() => removeWidget(id), [removeWidget, id]);
  const handleEditCustom = useCallback(() => openCustomEditor(id), [openCustomEditor, id]);
  const updateNodeInternals = useUpdateNodeInternals();

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

  const ports = getWidgetPorts(widget);
  // RawData 输入端口动态派生自连接边 (每个已连接的 source = 一个通道端口), 其余控件用静态定义
  const effectivePorts = widget.kind === 'RawData' ? deriveRawDataPorts(rfEdges, id) : ports;
  // 按控件类别着色 (与 WidgetPalette 分组颜色一致)
  const categoryColor = WIDGET_CATEGORY_COLORS[getWidgetCategory(widget.kind)];
  // 支持代码编辑的控件 — 节点头部显示编辑入口 (替代内嵌卡片的悬浮 ⚙)
  const editable = ['Gauge', 'LED', 'NumberDisplay', 'Custom', 'Math', 'Filter', 'FFT', 'IFFT'].includes(widget.kind);
  // 已连接的端口集合 — 用于 Handle 实色填充
  const connectedHandles = new Set<string>();
  for (const e of rfEdges) {
    if (e.source === id && e.sourceHandle) connectedHandles.add(e.sourceHandle);
    if (e.target === id && e.targetHandle) connectedHandles.add(e.targetHandle);
    // RawData 动态端口 id 是 `src:<sourceId>:<handle>` — 按 (source, sourceHandle) 标记已连接
    if (widget.kind === 'RawData' && e.target === id) connectedHandles.add(rawDataPortId(e.source, e.sourceHandle));
  }

  const renderContent = () => {
    switch (widget.kind) {
      case 'Knob':
        return <Knob widget={widget as Extract<WidgetConfig, { kind: 'Knob' }>} onRemove={onRemove} />;
      case 'Slider':
        return <Slider widget={widget as Extract<WidgetConfig, { kind: 'Slider' }>} onRemove={onRemove} />;
      case 'Button':
        return <ButtonWidget widget={widget as Extract<WidgetConfig, { kind: 'Button' }>} onRemove={onRemove} />;
      case 'Radio':
        return <Radio widget={widget as Extract<WidgetConfig, { kind: 'Radio' }>} onRemove={onRemove} />;
      case 'Checkbox':
        return <Checkbox widget={widget as Extract<WidgetConfig, { kind: 'Checkbox' }>} onRemove={onRemove} />;
      case 'Label':
        return <Label widget={widget as Extract<WidgetConfig, { kind: 'Label' }>} onRemove={onRemove} />;
      case 'PieChart':
        return <PieChart widget={widget as Extract<WidgetConfig, { kind: 'PieChart' }>} onRemove={onRemove} />;
      case 'Image':
        return <ImageViewer widget={widget as Extract<WidgetConfig, { kind: 'Image' }>} onRemove={onRemove} />;
      case 'Gauge':
        return <Gauge widget={widget as Extract<WidgetConfig, { kind: 'Gauge' }>} onRemove={onRemove} onEdit={handleEditCustom} />;
      case 'LED':
        return <LED widget={widget as Extract<WidgetConfig, { kind: 'LED' }>} onRemove={onRemove} onEdit={handleEditCustom} />;
      case 'NumberDisplay':
        return <NumberDisplay widget={widget as Extract<WidgetConfig, { kind: 'NumberDisplay' }>} onRemove={onRemove} onEdit={handleEditCustom} />;
      case 'Custom':
        return (
          <CustomWidget
            widget={widget as Extract<WidgetConfig, { kind: 'Custom' }>}
            onRemove={onRemove}
            onEdit={handleEditCustom}
            height={140}
          />
        );
      case 'Math':
        return (
          <MathWidget
            widget={widget as Extract<WidgetConfig, { kind: 'Math' }>}
            onRemove={onRemove}
            onEdit={handleEditCustom}
          />
        );
      case 'Filter':
        return (
          <FilterWidget
            widget={widget as Extract<WidgetConfig, { kind: 'Filter' }>}
            onRemove={onRemove}
            onEdit={handleEditCustom}
          />
        );
      case 'FFT':
        return (
          <FFTWidget
            widget={widget as Extract<WidgetConfig, { kind: 'FFT' }>}
            onRemove={onRemove}
            onEdit={handleEditCustom}
          />
        );
      case 'IFFT':
        return (
          <IFFTWidget
            widget={widget as Extract<WidgetConfig, { kind: 'IFFT' }>}
            onRemove={onRemove}
            onEdit={handleEditCustom}
          />
        );
      case 'TextDisplay':
        return (
          <TextDisplay
            widget={widget as Extract<WidgetConfig, { kind: 'TextDisplay' }>}
            onRemove={onRemove}
          />
        );
      case 'TextInput':
        return (
          <TextInput
            widget={widget as Extract<WidgetConfig, { kind: 'TextInput' }>}
            onRemove={onRemove}
          />
        );
      case 'Str':
        return (
          <StrWidget
            widget={widget as Extract<WidgetConfig, { kind: 'Str' }>}
            onRemove={onRemove}
          />
        );
      case 'Model3D':
    case 'Spectrum':
    case 'Waveform':
    case 'Command':
    case 'FrameDecoder':
    case 'RawData':
    case 'Trigger':
        // 这些控件在节点内仅显示占位, 实际渲染在数据窗口 (双击节点可打开/激活窗口)
        return (
          <div className="flex flex-col items-center gap-1 px-2 py-3 text-text-secondary text-[10px] text-center">
            <span>{widget.kind}</span>
            <span className="text-blue text-[9px]">↗ {t(lang, 'nodeOpenWindowHint')}</span>
          </div>
        );
      default:
        return null;
    }
  };

  // 获取 widget 显示名称 (LabelConfig 用 text, WaveformConfig 无 label 字段)
  const widgetLabel =
    widget.kind === 'Label'
      ? widget.params.text
      : 'label' in widget.params
      ? widget.params.label
      : widget.kind;

  return (
    <CanvasErrorTooltip message={errorMessage}>
      <div
        className="nowheel widget-card-acrylic rounded-md min-w-[160px] max-w-[240px] text-[11px] relative [&.selected]:border-accent"
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
      <div
        className="flex items-center justify-between px-1.5 py-1 border-b border-border text-[10px] font-semibold uppercase tracking-[0.4px]"
        style={{ color: categoryColor }}
      >
        <span className="flex-1 truncate" title={widget.kind}>
          {widgetLabel || widget.kind}
        </span>
        {editable && (
          <button
            className="w-4 h-4 p-0 opacity-60 hover:opacity-100 flex items-center justify-center rounded hover:bg-bg-hover transition-opacity"
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
          className="w-4 h-4 p-0 opacity-60 hover:opacity-100 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover transition-opacity"
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
        >
          <X size={10} />
        </button>
      </div>
      <div className="flex flex-row w-full min-h-[32px]">
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

        {/* 主内容区 */}
        <div className="flex-1 p-2 flex flex-col justify-center min-w-0">
          <div className="flex flex-col gap-1.5">
            <WidgetEmbeddedContext.Provider value={true}>
              {renderContent()}
            </WidgetEmbeddedContext.Provider>
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
