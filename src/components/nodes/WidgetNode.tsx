import { memo, useCallback, useEffect } from 'react';
import { Handle, Position, useUpdateNodeInternals, type NodeProps, type Edge } from '@xyflow/react';
import { useAppStore } from '../../store/appStore';
import { useDockStore } from '../../store/dockStore';
import { t } from '../../i18n';
import { X, Settings2 } from 'lucide-react';
import { WidgetEmbeddedContext } from '../ui/WidgetCard';
import type { WidgetConfig, DomainType } from '../../types';
import type { Lang } from '../../i18n';
import { UNARY_MATH_OPS, getWidgetCategory, WIDGET_CATEGORY_COLORS } from '../../types';
import { rawDataPortId } from '../../lib/utils/nodeDef';
import { commandInputPortNames } from '../../lib/utils/commandFrames';
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
import { CustomWidget, evalCustomWidgetDef } from '../displays/widgets/CustomWidget';
import { MathWidget } from '../displays/widgets/MathWidget';
import { FilterWidget } from '../displays/widgets/FilterWidget';
import { FFTWidget } from '../displays/widgets/FFTWidget';
import { IFFTWidget } from '../displays/widgets/IFFTWidget';
import { TextDisplay } from '../displays/widgets/TextDisplay';

/// 端口定义 — domain 标注该端口承载的是时域还是频域信号
export interface WidgetPort {
  id: string;
  label: string;
  domain: DomainType;
}

/// 获取模块的端口定义
export function getWidgetPorts(widget: WidgetConfig): {
  inputs: WidgetPort[];
  outputs: WidgetPort[];
} {
  switch (widget.kind) {
    case 'Knob':
    case 'Slider':
    case 'Button':
    case 'Radio':
    case 'Checkbox':
      // 输入控件: 只有输出端口
      return { inputs: [], outputs: [{ id: 'value', label: 'value', domain: 'time' }] };
    case 'Label':
    case 'Gauge':
    case 'LED':
    case 'NumberDisplay':
      // 显示控件: 只有单个输入端口
      return { inputs: [{ id: 'value', label: 'value', domain: 'time' }], outputs: [] };
    case 'PieChart':
      return {
        inputs: widget.params.segments.map((seg, i) => ({ id: `seg${i}`, label: seg, domain: 'time' as DomainType })),
        outputs: [],
      };
    case 'Image':
      return { inputs: [{ id: 'data', label: 'data', domain: 'time' }], outputs: [] };
    case 'Waveform':
      // 波形图: 多个通道输入端口
      return {
        inputs: Array.from({ length: widget.params.channels }, (_, i) => ({
          id: `CH${i}`,
          label: `CH${i}`,
          domain: 'time' as DomainType,
        })),
        outputs: [],
      };
    case 'Math': {
      // 算术控件: 多个输入端口 (单目运算固定 1 个) + 单输出
      const isUnary = UNARY_MATH_OPS.includes(widget.params.op);
      const inputCount = isUnary ? 1 : widget.params.inputCount;
      return {
        inputs: Array.from({ length: inputCount }, (_, i) => ({
          id: `in${i}`,
          label: `in${i}`,
          domain: 'time' as DomainType,
        })),
        outputs: [{ id: 'result', label: 'result', domain: 'time' }],
      };
    }
    case 'Filter':
      // 滤波器: 单输入 in0 (时域) + 单输出 result (时域)
      return {
        inputs: [{ id: 'in0', label: 'in0', domain: 'time' }],
        outputs: [{ id: 'result', label: 'result', domain: 'time' }],
      };
    case 'FFT':
      // FFT 频域求解器: 单输入 in0 (时域) + 单输出 spectrum (频域)
      return {
        inputs: [{ id: 'in0', label: 'in0', domain: 'time' }],
        outputs: [{ id: 'spectrum', label: 'spectrum', domain: 'freq' }],
      };
    case 'IFFT':
      // 逆 FFT 求解器: 单输入 spectrum (频域) + 单输出 out0 (时域)
      return {
        inputs: [{ id: 'spectrum', label: 'spectrum', domain: 'freq' }],
        outputs: [{ id: 'out0', label: 'out0', domain: 'time' }],
      };
    case 'Spectrum':
      // 频谱展示 (纯展示): 单输入 spectrum (频域) — 数据源由连线决定
      // (FFT 求解器的 spectrum 输出 → 本端口), 不再用下拉选择
      return { inputs: [{ id: 'spectrum', label: 'spectrum', domain: 'freq' }], outputs: [] };
    case 'Model3D':
      // 3D 模型: 六通道输入 (位置 x/y/z + 旋转 roll/pitch/yaw), 无输出 (前端 Three.js 直接渲染)
      // - trajectory          -> 仅使用 x/y/z (位置)
      // - attitude            -> 仅使用 roll/pitch/yaw (欧拉角, 弧度)
      // - trajectory-attitude -> 同时使用全部六通道
      // 缺失端口由 useGraphInputs 补 0, 向后兼容旧连线
      return {
        inputs: [
          { id: 'x', label: 'x', domain: 'time' },
          { id: 'y', label: 'y', domain: 'time' },
          { id: 'z', label: 'z', domain: 'time' },
          { id: 'roll', label: 'roll', domain: 'time' },
          { id: 'pitch', label: 'pitch', domain: 'time' },
          { id: 'yaw', label: 'yaw', domain: 'time' },
        ],
        outputs: [],
      };
    case 'Command': {
      // 命令发送: 输入端口 = 所有帧 var_ref 块 portName 并集 (端口名自定义)
      // loopbackOut 字节出口 — 发送的字节沿字节边路由 (Transport.tx 真实发送 / FrameDecoder.in 喂入)
      const inputs = commandInputPortNames(widget.params)
        .map((name) => ({ id: name, label: name, domain: 'time' as DomainType }));
      const outputs = [{ id: 'loopbackOut', label: 'loopbackOut', domain: 'bytes' as DomainType }];
      return { inputs, outputs };
    }
    case 'FrameDecoder': {
      // 帧解码器: 输出端口 = length/id/field/bitfield 块的 portName + 可选附加端口
      // 字节输入口 in (旧名 loopbackIn 后端仍兼容): 字节来源完全由输入字节边决定
      const blocks = widget.params.blocks ?? [];
      const inputs = [{ id: 'in', label: 'in', domain: 'bytes' as DomainType }];
      const outputs: WidgetPort[] = [];
      for (const b of blocks) {
        if (b.type === 'length') {
          const name = b.portName ?? 'length';
          outputs.push({ id: name, label: name, domain: 'time' });
        } else if (b.type === 'id') {
          const name = b.portName ?? 'id_value';
          outputs.push({ id: name, label: name, domain: 'time' });
        } else if (b.type === 'field' || b.type === 'bitfield') {
          outputs.push({ id: b.portName, label: b.portName, domain: 'time' });
        }
      }
      if (widget.params.enableValid) outputs.push({ id: 'valid', label: 'valid', domain: 'time' });
      if (widget.params.enableFrameCount) outputs.push({ id: 'frame_count', label: 'frame_count', domain: 'time' });
      if (widget.params.enableLastTimestamp) outputs.push({ id: 'last_timestamp', label: 'last_timestamp', domain: 'time' });
      if (widget.params.enableFps) outputs.push({ id: 'fps', label: 'fps', domain: 'time' });
      // raw 输出口: 整帧原始字节 (无 f32 语义) — 连到 RawData 时显示该解码器消费的完整帧字节;
      // 普通 field 口连 RawData 则显示该字段的数值流
      outputs.push({ id: 'raw', label: 'raw', domain: 'time' });
      return { inputs, outputs };
    }
    case 'Custom': {
      // Custom: 从用户代码中解析端口定义 (默认视为时域)
      const { def } = evalCustomWidgetDef(widget.params.code);
      return {
        inputs: (def?.inputs ?? [{ id: 'value', label: 'value' }]).map((p) => ({
          id: p.id,
          label: p.label,
          domain: 'time' as DomainType,
        })),
        outputs: (def?.outputs ?? []).map((p) => ({
          id: p.id,
          label: p.label,
          domain: 'time' as DomainType,
        })),
      };
    }
    case 'Trigger':
      // 触发器: 1 个数字触发端口 (auto 模式) + 2 个数值输出端口 (value / matched) + 1 个字符串输出端口 (text)
      return {
        inputs: [{ id: 'trigger', label: 'trigger', domain: 'time' }],
        outputs: [
          { id: 'value', label: 'value', domain: 'time' },
          { id: 'matched', label: 'matched', domain: 'time' },
          { id: 'text', label: 'text', domain: 'string' },
        ],
      };
    case 'TextDisplay':
      // 文本展示: 1 个字符串输入端口
      return {
        inputs: [{ id: 'text', label: 'text', domain: 'string' }],
        outputs: [],
      };
    case 'RawData':
      // 关联端口 (ASSOCIATIVE): 端口在此仅为回退值 — 实际端口由 WidgetNode 动态派生,
      // 每个已连接的 source 节点 = 一个通道端口。边只是用户意图标记: 控件视图展示
      // 选中通道的原始数据, 字节不路由进 f32 图 — 后端通过旁路通道捕获各解码器字节。
      return { inputs: [{ id: 'data', label: 'data', domain: 'time' }], outputs: [] };
    default:
      return { inputs: [{ id: 'in', label: 'in', domain: 'time' }], outputs: [] };
  }
}

/// 派生 RawData 输入端口 — 动态: 每条入边的 (source, sourceHandle) 组合 = 一个通道端口。
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

/// 端口域颜色 — 频域紫色, 时域蓝色, 字节域黄色 (仅作圆点/手柄描边, 不占文字宽度, 避免遮挡)
function domainColor(domain: DomainType): string {
  return domain === 'freq' ? '#ba68c8' : domain === 'bytes' ? '#e5c07b' : '#75beff';
}

/// 端口域标注文案 (悬停提示)
function domainLabel(lang: Lang, domain: DomainType): string {
  return domain === 'freq' ? t(lang, 'domainFreq') : domain === 'bytes' ? t(lang, 'domainBytes') : t(lang, 'domainTime');
}

/// 控件节点 — 包装实际控件, 添加 React Flow Handle
export const WidgetNode = memo(function WidgetNode({ id, data }: NodeProps) {
  const widget = data.widget as WidgetConfig | undefined;
  const removeWidget = useAppStore((s) => s.removeWidget);
  const openCustomEditor = useAppStore((s) => s.openCustomEditor);
  const rfEdges = useAppStore((s) => s.rfEdges);
  const lang = useAppStore((s) => s.lang);

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
    <div
      className="nowheel widget-card-acrylic rounded-md min-w-[160px] max-w-[240px] text-[11px] relative [&.selected]:border-accent"
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
      <div className="p-2 flex flex-col gap-1.5">
        <WidgetEmbeddedContext.Provider value={true}>
          {renderContent()}
        </WidgetEmbeddedContext.Provider>
      </div>
      {/* 输入端口 (左侧) — Handle 覆盖 position:relative 让多端口纵向分布 */}
      <div className="absolute top-1/2 left-0 -translate-y-1/2 flex flex-col gap-0.5 py-1">
        {effectivePorts.inputs.map((port) => (
          <div
            key={port.id}
            className="flex items-center gap-1 h-[14px] relative pl-0.5"
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
      {/* 输出端口 (右侧) — 标签在 Handle 左侧, 允许向左延伸适应过长端口名 */}
      <div className="absolute top-1/2 right-0 -translate-y-1/2 flex flex-col items-end gap-0.5 py-1 z-10">
        {effectivePorts.outputs.map((port) => (
          <div
            key={port.id}
            className="flex items-center gap-1 h-[14px] relative pr-0.5"
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
  );
});
