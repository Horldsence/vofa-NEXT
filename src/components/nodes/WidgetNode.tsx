import { Handle, Position, type NodeProps } from '@xyflow/react';
import { useAppStore } from '../../store/appStore';
import { X } from 'lucide-react';
import type { WidgetConfig } from '../../types';
import { UNARY_MATH_OPS } from '../../types';
import { Knob } from '../controls/Knob';
import { ButtonWidget } from '../controls/ButtonWidget';
import { Radio } from '../controls/Radio';
import { Checkbox } from '../controls/Checkbox';
import { Slider } from '../controls/Slider';
import { Label } from '../controls/Label';
import { PieChart } from '../displays/PieChart';
import { ImageViewer } from '../displays/ImageViewer';
import { Gauge } from '../displays/Gauge';
import { LED } from '../displays/LED';
import { NumberDisplay } from '../displays/NumberDisplay';
import { CustomWidget, evalCustomWidgetDef } from '../displays/CustomWidget';
import { MathWidget } from '../displays/MathWidget';
import { FilterWidget } from '../displays/FilterWidget';

/// 获取模块的端口定义
function getWidgetPorts(widget: WidgetConfig): {
  inputs: { id: string; label: string }[];
  outputs: { id: string; label: string }[];
} {
  switch (widget.kind) {
    case 'Knob':
    case 'Slider':
    case 'Button':
    case 'Radio':
    case 'Checkbox':
      // 输入控件: 只有输出端口
      return { inputs: [], outputs: [{ id: 'value', label: 'value' }] };
    case 'Label':
    case 'Gauge':
    case 'LED':
    case 'NumberDisplay':
      // 显示控件: 只有单个输入端口
      return { inputs: [{ id: 'value', label: 'value' }], outputs: [] };
    case 'PieChart':
      return {
        inputs: widget.params.segments.map((seg, i) => ({ id: `seg${i}`, label: seg })),
        outputs: [],
      };
    case 'Image':
      return { inputs: [{ id: 'data', label: 'data' }], outputs: [] };
    case 'Waveform':
      // 波形图: 多个通道输入端口
      return {
        inputs: Array.from({ length: widget.params.channels }, (_, i) => ({
          id: `CH${i}`,
          label: `CH${i}`,
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
        })),
        outputs: [{ id: 'result', label: 'result' }],
      };
    }
    case 'Filter':
      // 滤波器: 单输入 in0 + 单输出 result
      return {
        inputs: [{ id: 'in0', label: 'in0' }],
        outputs: [{ id: 'result', label: 'result' }],
      };
    case 'Spectrum':
      // 频谱分析: 单输入 in0, 无输出 (块运算, 后端独立 ticker 触发 FFT)
      return {
        inputs: [{ id: 'in0', label: 'in0' }],
        outputs: [],
      };
    case 'Model3D':
      // 3D 模型: 三通道输入 x/y/z, 无输出 (前端 Three.js 直接渲染)
      return {
        inputs: [
          { id: 'x', label: 'x' },
          { id: 'y', label: 'y' },
          { id: 'z', label: 'z' },
        ],
        outputs: [],
      };
    case 'Command': {
      // 命令发送: 从 blocks 中 var_ref 块推导输入端口 (端口名自定义)
      const blocks = widget.params.blocks ?? [];
      const inputs = blocks
        .filter((b) => b.type === 'var_ref' && b.portName)
        .map((b) => ({ id: b.portName!, label: b.portName! }));
      return { inputs, outputs: [] };
    }
    case 'FrameDecoder': {
      // 帧解码器: SOURCE 节点 — 输出端口 = length/id/field/bitfield 块的 portName + 可选附加端口
      // 无输入端口 (直接消费原始字节流, 由后端 data_loop 喂入)
      const blocks = widget.params.blocks ?? [];
      const outputs: { id: string; label: string }[] = [];
      for (const b of blocks) {
        if (b.type === 'length') {
          const name = b.portName ?? 'length';
          outputs.push({ id: name, label: name });
        } else if (b.type === 'id') {
          const name = b.portName ?? 'id_value';
          outputs.push({ id: name, label: name });
        } else if (b.type === 'field' || b.type === 'bitfield') {
          outputs.push({ id: b.portName, label: b.portName });
        }
      }
      if (widget.params.enableValid) outputs.push({ id: 'valid', label: 'valid' });
      if (widget.params.enableFrameCount) outputs.push({ id: 'frame_count', label: 'frame_count' });
      if (widget.params.enableLastTimestamp) outputs.push({ id: 'last_timestamp', label: 'last_timestamp' });
      if (widget.params.enableFps) outputs.push({ id: 'fps', label: 'fps' });
      return { inputs: [], outputs };
    }
    case 'Custom': {
      // Custom: 从用户代码中解析端口定义
      const { def } = evalCustomWidgetDef(widget.params.code);
      return {
        inputs: def?.inputs ?? [{ id: 'value', label: 'value' }],
        outputs: def?.outputs ?? [],
      };
    }
    default:
      return { inputs: [{ id: 'in', label: 'in' }], outputs: [] };
  }
}

/// 控件节点 — 包装实际控件, 添加 React Flow Handle
export function WidgetNode({ id, data }: NodeProps) {
  const widget = data.widget as WidgetConfig | undefined;
  const removeWidget = useAppStore((s) => s.removeWidget);
  const openCustomEditor = useAppStore((s) => s.openCustomEditor);

  if (!widget) {
    return <div className="p-2 text-red text-xs">Missing widget</div>;
  }

  const onRemove = () => removeWidget(id);
  const ports = getWidgetPorts(widget);

  const handleEditCustom = () => openCustomEditor(id);

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
      case 'Model3D':
    case 'Spectrum':
    case 'Waveform':
    case 'Command':
    case 'FrameDecoder':
        // 这些控件在节点内仅显示占位, 实际渲染在 DataPanel
        return (
          <div className="flex flex-col items-center gap-1 px-2 py-3 text-text-secondary text-[10px] text-center">
            <span>{widget.kind}</span>
            <span className="text-blue text-[9px]">→ DataPanel</span>
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
    <div className="bg-bg-sidebar border border-border rounded-md min-w-[160px] max-w-[240px] shadow-[0_2px_8px_rgba(0,0,0,0.4)] text-[11px] relative [&.selected]:border-accent [&.selected]:shadow-[0_0_0_1px_var(--accent),0_2px_12px_rgba(0,0,0,0.5)]">
      <div className="flex items-center justify-between px-1.5 py-1 bg-bg-panel-header border-b border-border rounded-t-md text-[10px] font-semibold uppercase tracking-[0.4px] text-text-secondary">
        <span className="flex-1 truncate" title={widget.kind}>
          {widgetLabel || widget.kind}
        </span>
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
      <div className="p-2 flex flex-col gap-1.5">{renderContent()}</div>
      {/* 输入端口 (左侧) — Handle 覆盖 position:relative 让多端口纵向分布 */}
      <div className="absolute top-1/2 left-0 -translate-y-1/2 flex flex-col gap-0.5 py-1">
        {ports.inputs.map((port) => (
          <div key={port.id} className="flex items-center gap-1 h-[14px] relative pl-0.5">
            <Handle
              type="target"
              position={Position.Left}
              id={port.id}
              style={{ position: 'relative', left: 'auto', top: 'auto', transform: 'none' }}
              className="w-[9px] h-[9px] bg-bg-input border-[1.5px] border-accent rounded-full cursor-crosshair transition-all duration-150 hover:bg-accent hover:scale-130 [&.connectingto]:bg-green [&.connectingto]:border-green [&.valid]:bg-green [&.valid]:border-green"
            />
            <span className="text-[9px] text-text-secondary font-mono whitespace-nowrap bg-bg-sidebar px-0.5 py-px rounded-sm">{port.label}</span>
          </div>
        ))}
      </div>
      {/* 输出端口 (右侧) — 标签在 Handle 左侧, 允许向左延伸适应过长端口名 */}
      <div className="absolute top-1/2 right-0 -translate-y-1/2 flex flex-col items-end gap-0.5 py-1 z-10">
        {ports.outputs.map((port) => (
          <div key={port.id} className="flex items-center gap-1 h-[14px] relative pr-0.5">
            <span className="text-[9px] text-text-secondary font-mono whitespace-nowrap bg-bg-sidebar px-0.5 py-px rounded-sm">{port.label}</span>
            <Handle
              type="source"
              position={Position.Right}
              id={port.id}
              style={{ position: 'relative', right: 'auto', top: 'auto', transform: 'none' }}
              className="w-[9px] h-[9px] bg-bg-input border-[1.5px] border-accent rounded-full cursor-crosshair transition-all duration-150 hover:bg-accent hover:scale-130 [&.connectingto]:bg-green [&.connectingto]:border-green [&.valid]:bg-green [&.valid]:border-green"
            />
          </div>
        ))}
      </div>
    </div>
  );
}
