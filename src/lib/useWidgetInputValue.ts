import { useState, useEffect } from 'react';
import { useAppStore } from '../store/appStore';
import { waveformWindow } from './dataBuffer';

/// 统一的控件输入值读取 hook
///
/// 优先级:
///   1. 如果有 edge 连接到本 widget 的 portId → 读取上游输出
///      - ChannelSource: 读 waveformWindow.channels[chIdx].last
///      - 其他 widget: 读 widgetOutputCache[sourceId][sourceHandle]
///   2. 否则, 如果 channel 参数不为 null → 读 waveformWindow.channels[channel].last
///   3. 否则返回 fallback (默认 0)
///
/// 每 50ms 轮询一次, 返回最新值
export function useWidgetInputValue(
  widgetId: string,
  portId: string = 'value',
  channel: number | null = null,
  fallback: number = 0
): number {
  const [value, setValue] = useState<number>(fallback);

  useEffect(() => {
    const tick = () => {
      const state = useAppStore.getState();
      const edges = state.rfEdges;
      const cache = state.widgetOutputCache;

      // 查找连到本 widget + 本 port 的 edge
      const edge = edges.find((e) => e.target === widgetId && e.targetHandle === portId);

      let next: number;
      if (edge) {
        const sourceHandle = edge.sourceHandle ?? 'value';
        const chMatch = /^ch(\d+)$/.exec(sourceHandle);
        if (chMatch) {
          // ChannelSource
          const chIdx = parseInt(chMatch[1], 10);
          const win = waveformWindow.get();
          const ch = win.channels[chIdx];
          next = ch && ch.length > 0 ? ch[ch.length - 1] : fallback;
        } else {
          // 上游 widget 输出
          next = cache[edge.source]?.[sourceHandle] ?? fallback;
        }
      } else if (channel !== null) {
        // 回退到 channel param
        const win = waveformWindow.get();
        const ch = win.channels[channel];
        next = ch && ch.length > 0 ? ch[ch.length - 1] : fallback;
      } else {
        next = fallback;
      }

      // 浅比较避免无谓的渲染
      setValue((prev) => (prev === next ? prev : next));
    };

    tick();
    const interval = setInterval(tick, 50);
    return () => clearInterval(interval);
  }, [widgetId, portId, channel, fallback]);

  return value;
}
