import { Handle, Position, type NodeProps } from '@xyflow/react';
import { useAppStore } from '../../store/appStore';
import { X } from 'lucide-react';
import type { WidgetConfig } from '../../types';
import { Knob } from '../controls/Knob';
import { ButtonWidget } from '../controls/ButtonWidget';
import { Radio } from '../controls/Radio';
import { Checkbox } from '../controls/Checkbox';
import { Slider } from '../controls/Slider';
import { Label } from '../controls/Label';
import { PieChart } from '../displays/PieChart';
import { ImageViewer } from '../displays/ImageViewer';

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
      // 显示控件: 只有输入端口
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
    default:
      return { inputs: [{ id: 'in', label: 'in' }], outputs: [] };
  }
}

/// 控件节点 — 包装实际控件, 添加 React Flow Handle
export function WidgetNode({ id, data }: NodeProps) {
  const widget = data.widget as WidgetConfig | undefined;
  const removeWidget = useAppStore((s) => s.removeWidget);

  if (!widget) {
    return <div className="rf-widget-node-error">Missing widget</div>;
  }

  const onRemove = () => removeWidget(id);
  const ports = getWidgetPorts(widget);

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
      case 'Waveform':
        // 波形图在节点内仅显示占位, 实际波形在 DataPanel 显示
        return (
          <div className="rf-waveform-placeholder">
            <span>{widget.params.channels} channels</span>
            <span className="rf-waveform-placeholder-hint">→ DataPanel</span>
          </div>
        );
      default:
        return null;
    }
  };

  return (
    <div className="rf-widget-node">
      <div className="rf-widget-node-header">
        <span className="rf-widget-node-kind">{widget.kind}</span>
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
