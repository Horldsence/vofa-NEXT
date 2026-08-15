import { useAppStore } from '../../store/appStore';
import { useShallow } from 'zustand/react/shallow';
import { waveformWindow } from '../buffers/dataBuffer';

/// 读取后端图评估后的输入值 (供显示控件使用)
///
/// 优先级:
///   1. 如果有 edge 连到本 widget 的 portId → 读取后端图输出 graphOutputs[sourceId][sourceHandle]
///      - ChannelSource: sourceHandle 形如 "ch0", 读 waveformWindow 最新值
///      - 其他 widget: 读 graphOutputs[sourceId][sourceHandle]
///   2. 否则, 如果 channel 参数不为 null → 读 waveformWindow.channels[channel].last
///   3. 否则返回 fallback
///
/// 窄订阅: 只订阅本 widget 关心的上游输出 (graphOutputs[source][handle]),
/// 值不变则不重渲染; 无 edge 且走 channel 回退时订阅 graphOutputsTick 跟随数据流。
/// 不再整图订阅 graphOutputs (原实现每个 RAF tick 重渲染所有标量 widget)。
export function useGraphInput(
  widgetId: string,
  portId: string = 'value',
  channel: number | null = null,
  fallback: number = 0
): number {
  // edges 仅在用户编辑图时变化, 整数组订阅代价可忽略
  const edges = useAppStore((s) => s.rfEdges);
  const edge = edges.find((e) => e.target === widgetId && e.targetHandle === portId);
  const source = edge?.source;
  const sourceHandle = edge?.sourceHandle ?? 'value';

  // 窄选择器: 只取本 widget 依赖的值 (基本类型, 严格相等即可抑制多余渲染)
  // 无 edge 走 channel 回退时, 值来自 waveformWindow (非响应式),
  // 退化为订阅 graphOutputsTick 以保持随数据流刷新
  const needTick = !source && channel !== null;
  const graphValue = useAppStore((s) =>
    source ? s.graphOutputs[source]?.[sourceHandle] : needTick ? s.graphOutputsTick : undefined
  );

  if (edge) {
    const chMatch = /^ch(\d+)$/.exec(sourceHandle);
    if (chMatch) {
      // ChannelSource: 读 waveformWindow 最新值 (波形数据独立于图输出)
      // graphValue (ChannelSource 的图输出) 随数据变化, 已驱动本组件重渲染
      const chIdx = parseInt(chMatch[1], 10);
      const win = waveformWindow.get();
      const ch = win.channels[chIdx];
      return ch && ch.length > 0 ? ch[ch.length - 1] : fallback;
    }
    // 上游 widget 输出: 从后端图快照读取
    return graphValue ?? fallback;
  }

  if (channel !== null) {
    // 回退到 channel 参数: 读 waveformWindow
    const win = waveformWindow.get();
    const ch = win.channels[channel];
    return ch && ch.length > 0 ? ch[ch.length - 1] : fallback;
  }

  return fallback;
}

/// 读取所有连到本 widget 的输入端口值 (用于多输入控件如 Math)
/// 返回 portId -> value 的映射
///
/// 窄订阅: useShallow 逐端口比较, 仅本 widget 的任一输入值变化时才重渲染
export function useGraphInputs(
  widgetId: string,
  portIds: string[],
  fallback: number = 0
): Record<string, number> {
  const edges = useAppStore((s) => s.rfEdges);

  // 每个 port 的上游 (source, handle); 无连接为 null
  const sources = portIds.map((portId) => {
    const edge = edges.find((e) => e.target === widgetId && e.targetHandle === portId);
    return edge ? { source: edge.source, handle: edge.sourceHandle ?? 'value' } : null;
  });

  // 窄选择器: 按端口取上游输出值, useShallow 逐元素比较 (number|undefined)
  const values = useAppStore(
    useShallow((s) =>
      sources.map((src) => (src ? s.graphOutputs[src.source]?.[src.handle] : undefined))
    )
  );

  const result: Record<string, number> = {};
  for (let i = 0; i < portIds.length; i++) {
    const src = sources[i];
    if (!src) {
      result[portIds[i]] = fallback;
      continue;
    }
    const chMatch = /^ch(\d+)$/.exec(src.handle);
    if (chMatch) {
      const chIdx = parseInt(chMatch[1], 10);
      const win = waveformWindow.get();
      const ch = win.channels[chIdx];
      result[portIds[i]] = ch && ch.length > 0 ? ch[ch.length - 1] : fallback;
    } else {
      result[portIds[i]] = values[i] ?? fallback;
    }
  }
  return result;
}
