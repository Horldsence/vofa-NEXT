import { useAppStore } from '../store/appStore';
import { waveformWindow } from './dataBuffer';

/// 读取后端图评估后的输入值 (供显示控件使用)
///
/// 优先级:
///   1. 如果有 edge 连到本 widget 的 portId → 读取后端图输出 graphOutputs[sourceId][sourceHandle]
///      - ChannelSource: sourceHandle 形如 "ch0", 读 waveformWindow 最新值
///      - 其他 widget: 读 graphOutputs[sourceId][sourceHandle]
///   2. 否则, 如果 channel 参数不为 null → 读 waveformWindow.channels[channel].last
///   3. 否则返回 fallback
///
/// 后端以 60 FPS 评估图并推送 graphOutputs, 此 hook 直接订阅 store 状态,
/// 无需轮询, 性能优于旧 useWidgetInputValue (50ms 轮询).
export function useGraphInput(
  widgetId: string,
  portId: string = 'value',
  channel: number | null = null,
  fallback: number = 0
): number {
  // 订阅 graphOutputs 和 rfEdges — 当后端推送新快照或 edges 变更时重新渲染
  const graphOutputs = useAppStore((s) => s.graphOutputs);
  const edges = useAppStore((s) => s.rfEdges);

  // 查找连到本 widget + 本 port 的 edge
  const edge = edges.find((e) => e.target === widgetId && e.targetHandle === portId);

  if (edge) {
    const sourceHandle = edge.sourceHandle ?? 'value';
    const chMatch = /^ch(\d+)$/.exec(sourceHandle);
    if (chMatch) {
      // ChannelSource: 读 waveformWindow 最新值 (波形数据独立于图输出)
      const chIdx = parseInt(chMatch[1], 10);
      const win = waveformWindow.get();
      const ch = win.channels[chIdx];
      return ch && ch.length > 0 ? ch[ch.length - 1] : fallback;
    }
    // 上游 widget 输出: 从后端图快照读取
    return graphOutputs[edge.source]?.[sourceHandle] ?? fallback;
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
export function useGraphInputs(
  widgetId: string,
  portIds: string[],
  fallback: number = 0
): Record<string, number> {
  const graphOutputs = useAppStore((s) => s.graphOutputs);
  const edges = useAppStore((s) => s.rfEdges);

  const result: Record<string, number> = {};
  for (const portId of portIds) {
    const edge = edges.find((e) => e.target === widgetId && e.targetHandle === portId);
    if (edge) {
      const sourceHandle = edge.sourceHandle ?? 'value';
      const chMatch = /^ch(\d+)$/.exec(sourceHandle);
      if (chMatch) {
        const chIdx = parseInt(chMatch[1], 10);
        const win = waveformWindow.get();
        const ch = win.channels[chIdx];
        result[portId] = ch && ch.length > 0 ? ch[ch.length - 1] : fallback;
      } else {
        result[portId] = graphOutputs[edge.source]?.[sourceHandle] ?? fallback;
      }
    } else {
      result[portId] = fallback;
    }
  }
  return result;
}
