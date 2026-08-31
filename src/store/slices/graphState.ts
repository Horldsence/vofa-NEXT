import type { SpectrumResult } from '../../types';

/// 前端专用快照状态；连续数值端口由 useNumeric* Hooks 直接订阅 DataBus。
export interface GraphStateSlice {
  /// 字符串输出平面 — 由 subscribeStringOutputs 推送
  /// key: widgetId, value: portId -> string
  customTextOutputs: Record<string, Record<string, string>>;
  customTextOutputsTick: number;
  spectrumResults: Record<string, SpectrumResult>;
  /// CAN 帧缓冲版本 (由 subscribeCanFrames 推送)
  canFramesVersion: number;
  /// 逻辑分析仪采样版本
  logicSamplesVersion: number;
}

export function createGraphStateSlice(): GraphStateSlice {
  return {
    customTextOutputs: {},
    customTextOutputsTick: 0,
    spectrumResults: {},
    canFramesVersion: 0,
    logicSamplesVersion: 0,
  };
}
