import { invoke } from '@tauri-apps/api/core';
import type { CanFrameBatch, CanFrame, CandleDeviceInfo } from '../../types';
import { makeOrderedSink, subscribeSharded } from './shardedSubscription';

/// 订阅 CAN 帧数据 — 统一分片流 (增量 drain, 首批回溯最近历史, 之后严格增量无重复)
/// 返回取消订阅函数
export function subscribeCanFrames(
  onEvent: (batch: CanFrameBatch) => void,
  options?: { intervalMs?: number; maxFrames?: number }
): { cancel: () => void } {
  return subscribeSharded<CanFrameBatch>(
    'subscribe_can_frames',
    'unsubscribe_can_frames',
    {},
    makeOrderedSink(onEvent),
    { intervalMs: options?.intervalMs, maxFrames: options?.maxFrames }
  );
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
