/// 节点定义 — 与 Rust vofa_next_nodes::NodeDef 对应
///
/// 用于 IPC: 前端把每个 tab 的 nodes + edges 通过 invoke('update_tab_graph') 同步到后端
/// 后端编译为 CompiledGraph, 在每帧数据到达时评估
///
/// 两层平面:
/// - 字节平面 (全局): Transport / Protocol 节点, 边携带 Vec<u8>, 事件驱动
/// - 数值平面 (每 tab): ProtocolSource 引用全局 Protocol 节点的最新帧, 输出 ch0..chN

import type { WidgetConfig, MathOp, WindowType, SpectrumOutput, DecoderBlock } from '../../types';
import type { TransportConfig, ProtocolConfig } from '../../types';
import { UNARY_MATH_OPS, biquadFromFilterConfig } from '../../types';
import { evalCustomWidgetDef } from '../../components/displays/widgets/CustomWidget';
import type { Edge } from '@xyflow/react';

/// Rust 端 NodeKind 序列化 — serde tag="kind" content="params"
///
/// FilterKind/IIR 系数使用 serde 默认 externally-tagged 表示:
///   { "IIR": { "b": [b0, b1, b2], "a": [a0, a1, a2] } }
/// WindowType/SpectrumOutput 是 unit variant: { "Hann": null }
/// FrameDecoder 子字段使用 snake_case (Rust 端无 rename_all), blocks 元素遵循 DecoderBlockDef 的 tag="type" + camelCase
/// Protocol.convert_to 为 None 时序列化省略 (Rust 侧 default + skip_serializing_if)
export type NodeKind =
  | { kind: 'Transport'; params: { config: TransportConfig } }
  | { kind: 'Protocol'; params: { config: ProtocolConfig; convert_to?: ProtocolConfig | null } }
  | { kind: 'ProtocolSource'; params: { node_id: string; channels: number } }
  | { kind: 'Input' }
  | { kind: 'Math'; params: { op: MathOp; input_count: number } }
  | { kind: 'Custom'; params: { inputs: string[]; outputs: string[] } }
  | { kind: 'Filter'; params: { kind: { IIR: { b: [number, number, number]; a: [number, number, number] } } } }
  | { kind: 'SpectrumSink'; params: { window_size: number; window_type: WindowType; output: SpectrumOutput; sample_rate: number } }
  | { kind: 'Ifft' }
  | { kind: 'FrameDecoder'; params: { blocks: DecoderBlock[]; enable_valid: boolean; enable_frame_count: boolean; enable_last_timestamp: boolean; enable_fps: boolean; loopback: boolean } }
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
/// - FFT → SpectrumSink { window_size, window_type, output, sample_rate } (频域求解器)
/// - IFFT → Ifft (逆 FFT 求解器, 频域→时域)
/// - Spectrum → Sink (纯展示, 从频谱数据通道读取 FFT 结果)
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

    case 'FFT': {
      // FFT 求解器 → 后端 SpectrumSink (消费时域样本, 输出频谱到专用频谱数据通道)
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

    case 'IFFT':
      // 逆 FFT 求解器 → 后端 Ifft 节点 (输入频域 spectrum, 输出时域 out0)
      return { kind: 'Ifft' };

    case 'Spectrum':
      // 频谱展示 (纯展示) → 无后端计算, 仅从频谱数据通道读取 FFT 求解器结果
    case 'Waveform':
    case 'PieChart':
    case 'Image':
    case 'Gauge':
    case 'LED':
    case 'NumberDisplay':
    case 'Label':
    case 'Model3D':
    case 'Command':
    case 'TableView':
    case 'RawData':
      return { kind: 'Sink' };

    case 'FrameDecoder': {
      return {
        kind: 'FrameDecoder',
        params: {
          blocks: widget.params.blocks,
          enable_valid: widget.params.enableValid,
          enable_frame_count: widget.params.enableFrameCount,
          enable_last_timestamp: widget.params.enableLastTimestamp,
          enable_fps: widget.params.enableFps,
          // 旧布局无该字段时按 false (默认接收实时 RX); 后端已忽略该字段, 仅为 serde 兼容保留
          loopback: widget.params.loopbackEnabled ?? false,
        },
      };
    }
  }
}

/// RawData 动态输入端口 id 约定: `src:<sourceId>:<sourceHandle>`
/// 每个已连接的 (source, sourceHandle) 组合 = 一个通道端口 (见 WidgetNode.deriveRawDataPorts)
export function rawDataPortId(sourceId: string, sourceHandle?: string | null): string {
  return `src:${sourceId}:${sourceHandle ?? 'data'}`;
}

/// 构造 ProtocolSource 节点的 NodeDef — tab 数值平面的帧源
/// id 与被引用的全局 Protocol 节点相同 (后端按 id 关联 source_frames)
export function makeProtocolSourceNodeDef(tabId: string, protocolNodeId: string, channels: number): NodeDef {
  return {
    id: protocolNodeId,
    tab_id: tabId,
    kind: { kind: 'ProtocolSource', params: { node_id: protocolNodeId, channels } },
  };
}

/// 构造 Transport 全局节点的 NodeDef
export function makeTransportNodeDef(tabId: string, nodeId: string, config: TransportConfig): NodeDef {
  return {
    id: nodeId,
    tab_id: tabId,
    kind: { kind: 'Transport', params: { config } },
  };
}

/// 构造 Protocol 全局节点的 NodeDef (convertTo 为 null 时省略序列化)
export function makeProtocolNodeDef(
  tabId: string,
  nodeId: string,
  config: ProtocolConfig,
  convertTo: ProtocolConfig | null
): NodeDef {
  return {
    id: nodeId,
    tab_id: tabId,
    kind: { kind: 'Protocol', params: { config, convert_to: convertTo ?? null } },
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
