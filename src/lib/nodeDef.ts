/// 节点定义 — 与 Rust vofa_next_nodes::NodeDef 对应
///
/// 用于 IPC: 前端把每个 tab 的 nodes + edges 通过 invoke('update_tab_graph') 同步到后端
/// 后端编译为 CompiledGraph, 在每帧数据到达时评估

import type { WidgetConfig, MathOp, WindowType, SpectrumOutput } from '../types';
import { UNARY_MATH_OPS, biquadFromFilterConfig } from '../types';
import { evalCustomWidgetDef } from '../components/displays/CustomWidget';
import { CHANNEL_SOURCE_ID } from '../store/appStore';
import type { Edge } from '@xyflow/react';

/// Rust 端 NodeKind 序列化 — serde tag="kind" content="params"
///
/// FilterKind/IIR 系数使用 serde 默认 externally-tagged 表示:
///   { "IIR": { "b": [b0, b1, b2], "a": [a0, a1, a2] } }
/// WindowType/SpectrumOutput 是 unit variant: { "Hann": null }
export type NodeKind =
  | { kind: 'ChannelSource'; params: { channels: number } }
  | { kind: 'Input' }
  | { kind: 'Math'; params: { op: MathOp; input_count: number } }
  | { kind: 'Custom'; params: { inputs: string[]; outputs: string[] } }
  | { kind: 'Filter'; params: { kind: { IIR: { b: [number, number, number]; a: [number, number, number] } } } }
  | { kind: 'SpectrumSink'; params: { window_size: number; window_type: WindowType; output: SpectrumOutput; sample_rate: number } }
  | { kind: 'Sink' };

/// 节点定义 DTO (IPC)
export interface NodeDef {
  id: string;
  tab_id: string;
  kind: NodeKind;
}

/// 从 WidgetConfig 推导 NodeKind (供 syncTabGraph 使用)
///
/// - Knob/Slider/Button/Radio/Checkbox → Input
/// - Math → Math { op, input_count }
/// - Custom → Custom { inputs, outputs } (从代码解析)
/// - Filter → Filter { kind: IIR { b, a } } (前端从 preset 计算 biquad 系数)
/// - Spectrum → SpectrumSink { window_size, window_type, output, sample_rate }
/// - Waveform/PieChart/Image/Gauge/LED/NumberDisplay/Label/Model3D/Command → Sink
///   (Command 的 value 输入端口由前端 useGraphInputs 读取, 用于模板插值)
export function widgetToNodeKind(widget: WidgetConfig): NodeKind {
  switch (widget.kind) {
    case 'Knob':
    case 'Slider':
    case 'Button':
    case 'Radio':
    case 'Checkbox':
      return { kind: 'Input' };

    case 'Math': {
      const isUnary = UNARY_MATH_OPS.includes(widget.params.op);
      return {
        kind: 'Math',
        params: {
          op: widget.params.op,
          input_count: isUnary ? 1 : widget.params.inputCount,
        },
      };
    }

    case 'Custom': {
      const { def } = evalCustomWidgetDef(widget.params.code);
      return {
        kind: 'Custom',
        params: {
          inputs: (def?.inputs ?? [{ id: 'value', label: 'value' }]).map((p) => p.id),
          outputs: (def?.outputs ?? []).map((p) => p.id),
        },
      };
    }

    case 'Filter': {
      const { b, a } = biquadFromFilterConfig(widget.params);
      return {
        kind: 'Filter',
        params: {
          kind: { IIR: { b, a } },
        },
      };
    }

    case 'Spectrum': {
      return {
        kind: 'SpectrumSink',
        params: {
          window_size: widget.params.windowSize,
          window_type: widget.params.windowType,
          output: widget.params.output,
          sample_rate: widget.params.sampleRate,
        },
      };
    }

    case 'Waveform':
    case 'PieChart':
    case 'Image':
    case 'Gauge':
    case 'LED':
    case 'NumberDisplay':
    case 'Label':
    case 'Model3D':
    case 'Command':
      return { kind: 'Sink' };
  }
}

/// 构造通道源节点的 NodeDef
export function makeChannelSourceNodeDef(tabId: string, channels: number): NodeDef {
  return {
    id: `${CHANNEL_SOURCE_ID}-${tabId}`,
    tab_id: tabId,
    kind: { kind: 'ChannelSource', params: { channels } },
  };
}

/// 边 DTO — 与 Rust vofa_next_buffer::graph::Edge 对应 (snake_case)
export interface GraphEdge {
  id: string;
  source: string;
  source_handle: string;
  target: string;
  target_handle: string;
}

/// 将 React Flow Edge (camelCase: sourceHandle/targetHandle) 转为后端 DTO (snake_case: source_handle/target_handle)
export function edgeToGraphEdge(edge: Edge): GraphEdge {
  return {
    id: edge.id,
    source: edge.source,
    source_handle: edge.sourceHandle ?? '',
    target: edge.target,
    target_handle: edge.targetHandle ?? '',
  };
}
