import { create } from 'zustand';
import type { ChannelMeasurementPayload, DerivedMeasurementPayload, ScopeAxisConfig } from '../types';
import { createDefaultScopeConfig } from '../types';

/// 后端测量流载荷 bundle — 按通道下标索引的原始载荷 (耦合展示换算在渲染层做)
export interface MeasurementsBundle {
  /// 本 bundle 对应的测量窗口 (毫秒) — 与订阅窗口一致
  windowMs: number;
  /// 快照来自金字塔层 (vavg/vrms 为包络中点近似; 极值仍精确)
  fromTier: boolean;
  /// 金字塔层序号 (fromTier 时有效)
  tierLevel: number;
  /// 按通道下标索引; 未测通道为 null
  channels: (ChannelMeasurementPayload | null)[];
  /// 派生序列测量 (MATH/Filter, 参与 AutoSet 周期检测)
  derived: DerivedMeasurementPayload[];
}

/// 每个 waveform widget 拥有独立的 axisConfig + measurements
/// 通过 widgetId 索引, 切换 Tab / 拆分成独立面板时配置跟随 widget, 互不干扰
///
/// 状态全部在前端 (内存态, 不持久化到后端 workspace):
/// - config: 时基/量程/游标等轴配置
/// - measurements: 后端测量流的最新快照 (按通道索引)
/// - measureChannel: 测量面板当前展示的通道 (null = 自动选第一个可见通道)
/// - autosetWarning: 最近一次 AutoSet 的钳位提示 (未完整显示目标周期数等)
export interface PerWidgetState {
  config: ScopeAxisConfig;
  measurements: MeasurementsBundle | null;
  measureChannel: number | null;
  autosetWarning: string | null;
}

/// 创建 per-widget state (懒初始化)
export function createPerWidgetState(channelCount: number): PerWidgetState {
  return {
    config: createDefaultScopeConfig(channelCount),
    measurements: null,
    measureChannel: null,
    autosetWarning: null,
  };
}

interface WaveformScopeStore {
  states: Record<string, PerWidgetState>;
  /// 确保 widget 配置存在且通道数足够
  ensureWidget: (widgetId: string, channelCount: number) => void;
  setConfig: (widgetId: string, channelCount: number, next: ScopeAxisConfig) => void;
  setMeasurements: (widgetId: string, channelCount: number, m: MeasurementsBundle) => void;
  setMeasureChannel: (widgetId: string, channelCount: number, channel: number | null) => void;
  setAutosetWarning: (widgetId: string, channelCount: number, warning: string | null) => void;
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

  setMeasurements: (widgetId, channelCount, m) =>
    set((prev) => {
      const cur = prev.states[widgetId];
      // 同一快照对象跳过重复写入 (测量流 version 门控后仍可能重放)
      if (cur?.measurements?.channels === m.channels) return prev;
      const next = cur ?? createPerWidgetState(channelCount);
      return {
        states: {
          ...prev.states,
          [widgetId]: { ...next, measurements: m },
        },
      };
    }),

  setMeasureChannel: (widgetId, channelCount, channel) =>
    set((prev) => {
      const cur = prev.states[widgetId] ?? createPerWidgetState(channelCount);
      if (cur.measureChannel === channel) return prev;
      return {
        states: { ...prev.states, [widgetId]: { ...cur, measureChannel: channel } },
      };
    }),

  setAutosetWarning: (widgetId, channelCount, warning) =>
    set((prev) => {
      const cur = prev.states[widgetId] ?? createPerWidgetState(channelCount);
      if (cur.autosetWarning === warning) return prev;
      return {
        states: { ...prev.states, [widgetId]: { ...cur, autosetWarning: warning } },
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
