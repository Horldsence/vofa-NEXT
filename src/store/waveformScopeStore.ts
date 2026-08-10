import { create } from 'zustand';
import type { ScopeAxisConfig, ScopeMeasurements } from '../types';
import { createDefaultScopeConfig } from '../types';

/// 每个 waveform widget 拥有独立的 axisConfig + measurements
/// 通过 widgetId 索引, 切换 Tab / 拆分成独立面板时配置跟随 widget, 互不干扰
export interface PerWidgetState {
  config: ScopeAxisConfig;
  measurements: ScopeMeasurements | null;
  lastMeasureVersion: number;
}

/// 创建 per-widget state (懒初始化)
export function createPerWidgetState(channelCount: number): PerWidgetState {
  return {
    config: createDefaultScopeConfig(channelCount),
    measurements: null,
    lastMeasureVersion: -1,
  };
}

interface WaveformScopeStore {
  states: Record<string, PerWidgetState>;
  /// 确保 widget 配置存在且通道数足够
  ensureWidget: (widgetId: string, channelCount: number) => void;
  setConfig: (widgetId: string, channelCount: number, next: ScopeAxisConfig) => void;
  setMeasurements: (
    widgetId: string,
    channelCount: number,
    version: number,
    m: ScopeMeasurements | null
  ) => void;
  /// 清理已移除 widget 的配置 (保留 default-waveform)
  pruneWidgets: (existingWidgetIds: string[]) => void;
}

export const useWaveformScopeStore = create<WaveformScopeStore>()((set) => ({
  states: { 'default-waveform': createPerWidgetState(4) },

  ensureWidget: (widgetId, channelCount) =>
    set((prev) => {
      const existing = prev.states[widgetId];
      if (existing) {
        if (existing.config.channels.length >= channelCount) return prev;
        const nextCh = existing.config.channels.slice();
        while (nextCh.length < channelCount) {
          nextCh.push({ vPerDiv: 1, position: 0, show: true, coupling: 'DC' });
        }
        return {
          states: {
            ...prev.states,
            [widgetId]: { ...existing, config: { ...existing.config, channels: nextCh } },
          },
        };
      }
      return { states: { ...prev.states, [widgetId]: createPerWidgetState(channelCount) } };
    }),

  setConfig: (widgetId, channelCount, next) =>
    set((prev) => {
      const cur = prev.states[widgetId] ?? createPerWidgetState(channelCount);
      return { states: { ...prev.states, [widgetId]: { ...cur, config: next } } };
    }),

  setMeasurements: (widgetId, channelCount, version, m) =>
    set((prev) => {
      const cur = prev.states[widgetId] ?? createPerWidgetState(channelCount);
      return {
        states: {
          ...prev.states,
          [widgetId]: { ...cur, lastMeasureVersion: version, measurements: m },
        },
      };
    }),

  pruneWidgets: (existingWidgetIds) =>
    set((prev) => {
      let changed = false;
      const next = { ...prev.states };
      for (const wid of Object.keys(next)) {
        if (wid === 'default-waveform') continue;
        if (!existingWidgetIds.includes(wid)) {
          delete next[wid];
          changed = true;
        }
      }
      return changed ? { states: next } : prev;
    }),
}));
