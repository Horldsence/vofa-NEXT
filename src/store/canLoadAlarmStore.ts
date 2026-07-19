import { create } from 'zustand';

/// CAN 负载告警阈值与状态的轻量 store
///
/// 设计: 独立于 appStore, 避免 StatusBar / CanLoadView 频繁 re-render 影响主 store。
/// CanLoadView 工具栏写入 threshold, StatusBar 内的 CanLoadAlarm 订阅读取。
/// 实时负载率由 CanLoadAlarm 自己的 subscribeCanLoad 推送更新, 不走此 store。
interface CanLoadAlarmState {
  /// 告警阈值 (0.0-1.0), 默认 0.7
  threshold: number;
  setThreshold: (v: number) => void;
  /// 是否启用告警 (默认 true)
  enabled: boolean;
  setEnabled: (v: boolean) => void;
}

export const useCanLoadAlarmStore = create<CanLoadAlarmState>((set) => ({
  threshold: 0.7,
  setThreshold: (v) => set({ threshold: Math.max(0, Math.min(1, v)) }),
  enabled: true,
  setEnabled: (v) => set({ enabled: v }),
}));
