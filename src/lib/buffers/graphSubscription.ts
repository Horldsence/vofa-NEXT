import { invoke } from '@tauri-apps/api/core';
import type { SpectrumResult } from '../../types';
import { subscribeDisplaySnapshot } from './shardedSubscription';

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
/// 30 FPS 推送, key = Fft widget id, value = 最新一次 FFT 结果
export interface SpectrumBatch {
  /// sink widget id -> 频谱结果
  spectra: Record<string, SpectrumResult>;
}

/// 订阅图输出快照 (60 FPS 推送)
/// 返回取消订阅函数
export function subscribeGraphOutputs(
  onEvent: (snapshot: GraphOutputSnapshot) => void
): { cancel: () => void } {
  return subscribeDisplaySnapshot({ kind: 'graph_outputs' }, 'graph_outputs', onEvent, 16);
}

/// 订阅 Custom widget 输入批次 (30 FPS 推送)
export function subscribeCustomInputs(
  onEvent: (batch: CustomInputBatch) => void
): { cancel: () => void } {
  return subscribeDisplaySnapshot({ kind: 'custom_inputs' }, 'custom_inputs', onEvent, 33);
}

/// 订阅频谱分析结果 (30 FPS 推送)
/// batch.spectra: sinkWidgetId -> SpectrumResult
export function subscribeSpectrum(
  onEvent: (batch: SpectrumBatch) => void
): { cancel: () => void } {
  return subscribeDisplaySnapshot(
    { kind: 'spectrum' },
    'spectrum',
    (spectra: Record<string, SpectrumResult>) => onEvent({ spectra }),
    33
  );
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

/// 提交字符串输出 — Custom JS widget 字符串输出回传通道
/// (Trigger 的字符串规则输出已由后端图求值直接产出, 不再走此命令; 当前前端无调用方)
/// 后端写入 custom_text_outputs 并经 text_output_ticker 推送给订阅者 (TextDisplay)
export function submitCustomTextOutput(
  widgetId: string,
  outputs: Record<string, string>
): Promise<void> {
  return invoke('submit_custom_text_output', { widgetId, outputs });
}

/// 字符串输出快照 — 与 GraphOutputSnapshot 平行的字符串平面
export interface StringOutputSnapshot {
  tick: number;
  values: Record<string, Record<string, string>>;
}

/// 订阅字符串输出快照 — 30 FPS 自适应推送 (镜像 subscribeGraphOutputs)
export function subscribeStringOutputs(
  onEvent: (snapshot: StringOutputSnapshot) => void
): { cancel: () => void } {
  return subscribeDisplaySnapshot({ kind: 'string_outputs' }, 'string_outputs', onEvent, 33);
}
