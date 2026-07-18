import { invoke, Channel } from '@tauri-apps/api/core';
import type { CanFrameBatch, CanFrame, CandleDeviceInfo } from '../types';
import { closeTauriChannel } from './tauri';

/// 订阅 CAN 帧数据 (后端周期性推送 can_buffer 中的最近帧)
/// 返回取消订阅函数
export function subscribeCanFrames(
  onEvent: (batch: CanFrameBatch) => void,
  options?: { intervalMs?: number; maxFrames?: number }
): { cancel: () => void } {
  const channel = new Channel<CanFrameBatch>();
  channel.onmessage = onEvent;
  void invoke('subscribe_can_frames', {
    onEvent: channel,
    intervalMs: options?.intervalMs,
    maxFrames: options?.maxFrames,
  });
  return {
    cancel: () => {
      void closeTauriChannel(channel, 'unsubscribe_can_frames', channel.id);
    },
  };
}

/// 发送 CAN 帧
export function sendCanFrame(frame: CanFrame): Promise<void> {
  return invoke('send_can_frame', { frame });
}

/// 同步查询: 获取最近 N 个 CAN 帧
export function getRecentCanFrames(count: number): Promise<CanFrameBatch> {
  return invoke('get_recent_can_frames', { count });
}

/// 清空 CAN 帧缓冲区
export function clearCanBuffer(): Promise<void> {
  return invoke('clear_can_buffer');
}

/// 获取 CAN 缓冲区当前帧数
export function getCanBufferInfo(): Promise<number> {
  return invoke('get_can_buffer_info');
}

/// 列举所有 candleLight USB 设备
export function listCandleDevices(): Promise<CandleDeviceInfo[]> {
  return invoke('list_candle_devices');
}
