import { useEffect } from 'react';
import type { ChannelMeasurementPayload, DerivedMeasurementPayload } from '../../types';
import type { DerivedSeriesSelector } from '../../types';
import { api } from '../tauri/tauri';
import { useWaveformScopeStore } from '../../store/waveformScopeStore';

interface UseWaveformMeasurementsOptions {
  sourceId: string | null;
  /** 测量窗口 (毫秒) — 随时基变化, 防抖重订阅 */
  windowMs: number;
  /** 参与测量的派生序列选择器 (与波形图 series 一致; JSON 稳定) */
  derivedSelectors: DerivedSeriesSelector[];
  widgetId: string;
  channelCount: number;
}

/**
 * 后端测量流订阅 — 统计/周期全部由后端在权威缓冲 (金字塔快照) 上计算,
 * 前端零数据计算。原始载荷按通道索引写入 waveformScopeStore (前端持有的状态),
 * 耦合展示换算在渲染层完成 (纯算术, 见 lib/utils/measureDisplay)。
 *
 * 测量与信号来源彻底解耦: 只依赖 (sourceId, windowMs), 真实设备 / TestData /
 * 回放行为一致; 与波形显示缓冲不共享任何数据路径。
 */
export function useWaveformMeasurements({
  sourceId,
  windowMs,
  derivedSelectors,
  widgetId,
  channelCount,
}: UseWaveformMeasurementsOptions): void {
  // 选择器按 JSON 内容稳定 — 连线变化才重建订阅
  const derivedKey = JSON.stringify(derivedSelectors);
  // 75ms 防抖 (与 detail 流一致) — 时基快速变化时不频繁重建订阅
  useEffect(() => {
    if (!sourceId || !(windowMs > 0)) return;
    let cancel: (() => void) | null = null;
    const timer = window.setTimeout(() => {
      const subscription = api.subscribeMeasurements(
        sourceId,
        windowMs,
        JSON.parse(derivedKey) as DerivedSeriesSelector[],
        (payload) => {
          const channels: (ChannelMeasurementPayload | null)[] = [];
          for (let i = 0; i < Math.max(channelCount, payload.channels.length); i++) {
            channels.push(
              payload.channels.find((c) => c.channel === i) ?? null,
            );
          }
          const derived: DerivedMeasurementPayload[] = payload.derived ?? [];
          useWaveformScopeStore.getState().setMeasurements(widgetId, channelCount, {
            windowMs: payload.window_ms,
            fromTier: payload.from_tier,
            tierLevel: payload.tier_level,
            channels,
            derived,
          });
        },
        100,
      );
      cancel = subscription.cancel;
    }, 75);
    return () => {
      window.clearTimeout(timer);
      cancel?.();
    };
  }, [sourceId, windowMs, derivedKey, widgetId, channelCount]);
}
