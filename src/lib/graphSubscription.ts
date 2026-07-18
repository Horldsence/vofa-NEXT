import { invoke, Channel } from '@tauri-apps/api/core';
import type { SpectrumResult } from '../types';
import { closeTauriChannel } from './tauri';

/// 后端图输出快照 — 与 Rust GraphOutputSnapshot 对应
export interface GraphOutputSnapshot {
  tick: number;
  /// widgetId -> portId -> value
  values: Record<string, Record<string, number>>;
}

/// Custom widget 输入批次 — 与 Rust CustomInputBatch 对应
export interface CustomInputBatch {
  /// custom widget id -> input port id -> value
  inputs: Record<string, Record<string, number>>;
}

/// 频谱批次 — 与 Rust SpectrumBatch 对应
/// 30 FPS 推送, key = SpectrumSink widget id, value = 最新一次 FFT 结果
export interface SpectrumBatch {
  /// sink widget id -> 频谱结果
  spectra: Record<string, SpectrumResult>;
}

/// 订阅图输出快照 (60 FPS 推送)
/// 返回取消订阅函数
export function subscribeGraphOutputs(
  onEvent: (snapshot: GraphOutputSnapshot) => void
): { cancel: () => void } {
  const channel = new Channel<GraphOutputSnapshot>();
  channel.onmessage = onEvent;
  void invoke('subscribe_graph_outputs', { onEvent: channel });
  return {
    cancel: () => {
      void closeTauriChannel(channel, 'unsubscribe_graph_outputs', channel.id);
    },
  };
}

/// 订阅 Custom widget 输入批次 (30 FPS 推送)
export function subscribeCustomInputs(
  onEvent: (batch: CustomInputBatch) => void
): { cancel: () => void } {
  const channel = new Channel<CustomInputBatch>();
  channel.onmessage = onEvent;
  void invoke('subscribe_custom_inputs', { onEvent: channel });
  return {
    cancel: () => {
      void closeTauriChannel(channel, 'unsubscribe_custom_inputs', channel.id);
    },
  };
}

/// 订阅频谱分析结果 (30 FPS 推送)
/// batch.spectra: sinkWidgetId -> SpectrumResult
export function subscribeSpectrum(
  onEvent: (batch: SpectrumBatch) => void
): { cancel: () => void } {
  const channel = new Channel<SpectrumBatch>();
  channel.onmessage = onEvent;
  void invoke('subscribe_spectrum', { onEvent: channel });
  return {
    cancel: () => {
      void closeTauriChannel(channel, 'unsubscribe_spectrum', channel.id);
    },
  };
}

/// 设置输入控件当前值 (Knob/Slider/Button/Radio/Checkbox 拖动时调用)
export function setInputValue(widgetId: string, value: number): Promise<void> {
  return invoke('set_input_value', { widgetId, value });
}

/// 提交 Custom widget 输出 (iframe 调用 ctx.send 后回传)
export function submitCustomOutput(
  widgetId: string,
  outputs: Record<string, number>
): Promise<void> {
  return invoke('submit_custom_output', { widgetId, outputs });
}
