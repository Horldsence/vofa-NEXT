import type { Node, Edge } from '@xyflow/react';
import type { WidgetConfig } from '../../types';
import { traceTransportSource } from '../../store/appStoreHelpers';
import { rawDataPortId } from './nodeDef';
import { getWidgetPorts } from '../../components/nodes/WidgetPorts';

/// RawData 通道种类:
/// - decoder-node: FrameDecoder 的 raw 口 → 节点旁路收集器 (该解码器每帧消费的整帧字节)
/// - byte-source:  字节平面源 (Transport rx / Protocol out) → 上游 Transport 的原始收发字节流;
///   Protocol 的 str 口 (RawData 预设字符串行) 同样按原始字节渲染
/// - string:       字符串域输出口 (Trigger.text / TextInput.str / Str 字符串类 op) →
///   字符串平面 (graph_string_outputs) 的值变化历史
/// - numeric:      其余数值源 (含 Protocol 的 chN 数值口) → graphOutputs 数值流
export type RawDataChannelKind = 'decoder-node' | 'byte-source' | 'string' | 'numeric';

export interface RawDataChannelInfo {
  kind: RawDataChannelKind;
  /// byte-source 通道的字节源 Transport 节点 id; 上溯失败为 null (通道显示为空, 不订阅)
  transportId: string | null;
}

/// 分类 RawData 控件的通道 (一条入边 = 一个通道), 决定该通道的数据来源
export function classifyRawDataChannel(
  channel: { sourceId: string; sourceHandle?: string },
  nodes: Node[],
  edges: Edge[],
  widgets: WidgetConfig[]
): RawDataChannelInfo {
  if (
    channel.sourceHandle === 'raw' &&
    widgets.some((w) => w.kind === 'FrameDecoder' && w.params.id === channel.sourceId)
  ) {
    return { kind: 'decoder-node', transportId: null };
  }
  const sourceNode = nodes.find((n) => n.id === channel.sourceId);
  if (sourceNode?.type === 'transport') {
    return {
      kind: 'byte-source',
      transportId: traceTransportSource(channel.sourceId, edges, nodes),
    };
  }
  if (sourceNode?.type === 'protocol') {
    // 协议节点按端口分流: out (字节出口) 与 str (RawData 预设字符串行) 沿字节
    // 平面渲染原始流; chN 数值口是解析后的 f32 采样, 走 graphOutputs 数值流 —
    // 不能混同, 否则数值通道显示的是输入端的原始帧字节
    const bytePlanePort = channel.sourceHandle === 'out' || channel.sourceHandle === 'str';
    if (bytePlanePort) {
      return {
        kind: 'byte-source',
        transportId: traceTransportSource(channel.sourceId, edges, nodes),
      };
    }
    return { kind: 'numeric', transportId: null };
  }
  // 控件字符串域输出口 (端口表单一权威: getWidgetPorts) → 字符串平面历史
  const sourceWidget = widgets.find((w) => w.params.id === channel.sourceId);
  if (sourceWidget) {
    try {
      const output = getWidgetPorts(sourceWidget).outputs.find(
        (p) => p.id === (channel.sourceHandle ?? 'data')
      );
      if (output?.domain === 'string') {
        return { kind: 'string', transportId: null };
      }
    } catch {
      // Custom widget 代码求值失败 → 回退数值通道
    }
  }
  return { kind: 'numeric', transportId: null };
}

/// 纯端口制选择解析 — RawData 卡片的输入选择单一事实源:
/// - 配置选中 (`selectedInput`) 且该连线仍存在 → 保持用户选择 (每张卡片独立)
/// - 缺省/失效 (连线删除、图变更) → 回退第一个已连接端口
/// - 无任何连线 → null (视图渲染空态引导)
export function resolveRawDataChannelKey(
  selectedInput: string | undefined,
  options: readonly { key: string }[]
): string | null {
  if (options.length === 0) return null;
  if (selectedInput && options.some((o) => o.key === selectedInput)) return selectedInput;
  return options[0].key;
}

/// RawData 卡片状态提示用 — 解析当前生效输入对应的可观察 Transport
///
/// 返回 Transport 节点 id (供读 `connectionStates` 显示 未连接/错误 状态), 以下情形返回 null:
/// - 无任何连线 (空态, 视图已有专门引导)
/// - 生效输入是 FrameDecoder 的 raw 口 (数据来自节点旁路收集器, 无固定连接语义)
export function resolveRawDataStatusTransport(
  widgetId: string,
  selectedInput: string | undefined,
  edges: Edge[],
  nodes: Node[],
  widgets: WidgetConfig[]
): string | null {
  // 通道选项派生与 RawDataView.channelOptions 同源: 入边 (source, sourceHandle) 去重
  const seen = new Set<string>();
  const options: { key: string }[] = [];
  for (const e of edges) {
    if (e.target !== widgetId) continue;
    const key = rawDataPortId(e.source, e.sourceHandle);
    if (seen.has(key)) continue;
    seen.add(key);
    options.push({ key });
  }
  const key = resolveRawDataChannelKey(selectedInput, options);
  if (!key) return null;
  // key 形如 `src:<sourceId>:<handle>` (rawDataPortId 约定; handle 可含冒号, 取首段)
  const sourceId = key.slice('src:'.length).split(':')[0];
  if (widgets.some((w) => w.kind === 'FrameDecoder' && w.params.id === sourceId)) {
    return null;
  }
  return traceTransportSource(sourceId, edges, nodes);
}
