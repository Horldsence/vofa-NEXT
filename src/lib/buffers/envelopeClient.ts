import { invoke, Channel } from '@tauri-apps/api/core';
import { closeTauriChannel } from '../tauri/tauri';
import { decodeEnvelopeFrame, type WaveformEnvelopeFrame } from '../data/envelopeProtocol';

/// 订阅波形包络 (后端逐列 min/max 降采样, GPU 加速 + CPU 回退)。
///
/// 二进制 VENV 帧经 Tauri Channel 直达 (InvokeResponseBody::Raw → ArrayBuffer),
/// 与 PortSamples 同通道模式; 缎带绘制见 WaveformEnvelopeChart。
/// 返回取消订阅函数。
export function subscribeWaveformEnvelope(
  source: string,
  columns: number,
  onEnvelope: (frame: WaveformEnvelopeFrame) => void,
  options?: { intervalMs?: number },
): { cancel: () => void } {
  const channel = new Channel<ArrayBuffer>();
  channel.onmessage = (buffer) => {
    try {
      onEnvelope(decodeEnvelopeFrame(buffer));
    } catch (err) {
      console.error('VENV envelope decode failed:', err);
    }
  };
  void invoke('subscribe_data', {
    request: { kind: 'waveform_envelope', source, columns },
    onEvent: channel,
    intervalMs: options?.intervalMs ?? 33,
    maxItems: null,
  });
  return {
    cancel: () =>
      void closeTauriChannel(channel, 'unsubscribe_data', channel.id),
  };
}
