import type { SpectrumResult } from '../../types';

/// 后端图评估状态 — 由 subscribeGraphOutputs / subscribeCustomInputs / subscribeSpectrum 推送
export interface GraphStateSlice {
  graphOutputs: Record<string, Record<string, number>>;
  graphOutputsTick: number;
  customInputs: Record<string, Record<string, number>>;
  spectrumResults: Record<string, SpectrumResult>;
  /// CAN 帧缓冲版本 (由 subscribeCanFrames 推送)
  canFramesVersion: number;
  /// 逻辑分析仪采样版本
  logicSamplesVersion: number;
}

export function createGraphStateSlice(): GraphStateSlice {
  return {
    graphOutputs: {},
    graphOutputsTick: 0,
    customInputs: {},
    spectrumResults: {},
    canFramesVersion: 0,
    logicSamplesVersion: 0,
  };
}
