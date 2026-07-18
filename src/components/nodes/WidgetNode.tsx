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
    case 'Command':
      // 命令发送: 无输入端口 (主动发送), 无输出 (前端 → transport)
      return { inputs: [], outputs: [] };
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
    return <div className="rf-widget-node-error">Missing widget</div>;
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
        // 这些控件在节点内仅显示占位, 实际渲染在 DataPanel
        return (
          <div className="rf-waveform-placeholder">
            <span>{widget.kind}</span>
            <span className="rf-waveform-placeholder-hint">→ DataPanel</span>
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
    <div className="rf-widget-node">
      <div className="rf-widget-node-header">
        <span className="rf-widget-node-kind" title={widget.kind}>
          {widgetLabel || widget.kind}
        </span>
        <button
          className="btn-icon rf-widget-node-close"
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
        >
          <X size={10} />
        </button>
      </div>
      <div className="rf-widget-node-body">{renderContent()}</div>
      {/* 输入端口 (左侧) */}
      <div className="rf-ports-left">
        {ports.inputs.map((port) => (
          <div key={port.id} className="rf-port-row">
            <Handle
              type="target"
              position={Position.Left}
              id={port.id}
              className="rf-handle rf-handle-target"
            />
            <span className="rf-port-label">{port.label}</span>
          </div>
        ))}
      </div>
      {/* 输出端口 (右侧) */}
      <div className="rf-ports-right">
        {ports.outputs.map((port) => (
          <div key={port.id} className="rf-port-row">
            <span className="rf-port-label">{port.label}</span>
            <Handle
              type="source"
              position={Position.Right}
              id={port.id}
              className="rf-handle rf-handle-source"
            />
          </div>
        ))}
      </div>
    </div>
  );
}
