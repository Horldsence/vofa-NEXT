import { invoke } from '@tauri-apps/api/core';
import type { CanFrameBatch, CanFrame, CanDirection, CandleDeviceInfo } from '../../types';
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

/// CAN 帧过滤条件 — 与 Rust CanFrameFilter 对应 (全部缺省 = 匹配全部帧)
export interface CanFrameFilterOptions {
  /// 精确 ID 匹配
  id?: number;
  /// ID 掩码匹配 (id & mask == filter.id & mask)
  id_mask?: number;
  /// ID 范围下限 (含)
  id_min?: number;
  /// ID 范围上限 (含)
  id_max?: number;
  /// 扩展帧过滤
  extended?: boolean;
  /// 远程帧过滤
  rtr?: boolean;
  /// 方向过滤
  direction?: CanDirection;
  /// 数据内容子串匹配 (字节序列)
  data_pattern?: number[];
}

/// 订阅带过滤条件的 CAN 帧 — 统一分片流
///
/// 后端只推送匹配 filter 的帧; 游标从最旧可读位置开始,
/// 先拉取全部历史匹配帧, 之后严格增量。前端无需再遍历过滤。
export function subscribeCanFramesFiltered(
  filter: CanFrameFilterOptions,
  onEvent: (batch: CanFrameBatch) => void,
  options?: { intervalMs?: number; maxFrames?: number }
): { cancel: () => void } {
  return subscribeSharded<CanFrameBatch>(
    'subscribe_can_frames_filtered',
    'unsubscribe_can_frames',
    { filter },
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
