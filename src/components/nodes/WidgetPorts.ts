/// 控件节点端口表 — WidgetConfig → WidgetPort[] 派生 (单一权威)
/// 与 WidgetNode 解耦: WidgetNode 只消费 getWidgetPorts 的输出 (端口 id/label/domain)
/// 端口表按 WidgetConfig.kind 分发; StrOp / Command 等动态端口表来自各 crate 已 export 的 helper

import type { WidgetConfig, DomainType } from '../../types';
import { isUnaryMathOp, STR_OP_PORTS } from '../../types';
import { commandInputPortNames } from '../../lib/utils/commandFrames';
import { evalCustomWidgetDef } from '../displays/widgets/CustomWidget';

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
  switch ( widget.kind) {
    case 'Knob':
    case 'Slider':
    case 'Button':
    case 'Radio':
    case 'Checkbox':
      return { inputs: [], outputs: [{ id: 'value', label: 'value', domain: 'time' }] };
    case 'Label':
    case 'Gauge':
    case 'LED':
    case 'NumberDisplay':
      return { inputs: [{ id: 'value', label: 'value', domain: 'time' }], outputs: [] };
    case 'PieChart':
      return {
        inputs: widget.params.segments.map((seg, i) => ({ id: `seg${i}`, label: seg, domain: 'time' as DomainType })),
        outputs: [],
      };
    case 'Image':
      return { inputs: [{ id: 'data', label: 'data', domain: 'time' }], outputs: [] };
    case 'Waveform':
      return {
        inputs: Array.from({ length: widget.params.channels }, (_, i) => ({
          id: `CH${i}`,
          label: `CH${i}`,
          domain: 'time' as DomainType,
        })),
        outputs: [],
      };
    case 'Math': {
      const isUnary = isUnaryMathOp(widget.params.op);
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
      return {
        inputs: [{ id: 'in0', label: 'in0', domain: 'time' }],
        outputs: [{ id: 'result', label: 'result', domain: 'time' }],
      };
    case 'FFT':
      return {
        inputs: [{ id: 'in0', label: 'in0', domain: 'time' }],
        outputs: [{ id: 'spectrum', label: 'spectrum', domain: 'freq' }],
      };
    case 'IFFT':
      return {
        inputs: [{ id: 'spectrum', label: 'spectrum', domain: 'freq' }],
        outputs: [{ id: 'out0', label: 'out0', domain: 'time' }],
      };
    case 'Spectrum':
      return { inputs: [{ id: 'spectrum', label: 'spectrum', domain: 'freq' }], outputs: [] };
    case 'Model3D':
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
      const inputs = commandInputPortNames(widget.params)
        .map((name: string) => ({ id: name, label: name, domain: 'time' as DomainType }));
      const outputs = [{ id: 'loopbackOut', label: 'loopbackOut', domain: 'bytes' as DomainType }];
      return { inputs, outputs };
    }
    case 'FrameDecoder': {
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
      outputs.push({ id: 'raw', label: 'raw', domain: 'time' });
      return { inputs, outputs };
    }
    case 'Custom': {
      const { def } = evalCustomWidgetDef(widget.params.code);
      return {
        inputs: (def?.inputs ?? [{ id: 'value', label: 'value' }]).map((p: { id: string; label: string }) => ({
          id: p.id,
          label: p.label,
          domain: 'time' as DomainType,
        })),
        outputs: (def?.outputs ?? []).map((p: { id: string; label: string }) => ({
          id: p.id,
          label: p.label,
          domain: 'time' as DomainType,
        })),
      };
    }
    case 'Trigger':
      return {
        inputs: [{ id: 'trigger', label: 'trigger', domain: 'time' }],
        outputs: [
          { id: 'value', label: 'value', domain: 'time' },
          { id: 'matched', label: 'matched', domain: 'time' },
          { id: 'text', label: 'text', domain: 'string' },
        ],
      };
    case 'TextInput':
      return {
        inputs: [],
        outputs: [{ id: 'str', label: 'str', domain: 'string' }],
      };
    case 'TextDisplay':
      return {
        inputs: [{ id: 'text', label: 'text', domain: 'string' }],
        outputs: [],
      };
    case 'Str': {
      const meta = STR_OP_PORTS[widget.params.op];
      return {
        inputs: meta.inputs.map((p: WidgetPort) => ({ ...p })),
        outputs: [{ id: 'result', label: 'result', domain: meta.outputDomain }],
      };
    }
    case 'RawData':
      return { inputs: [{ id: 'data', label: 'data', domain: 'time' }], outputs: [] };
    default:
      return { inputs: [{ id: 'in', label: 'in', domain: 'time' }], outputs: [] };
  }
}