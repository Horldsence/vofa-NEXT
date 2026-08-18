//! 按源 (节点 id) 管理波形 / 原始数据订阅与前端缓冲
//!
//! 后端重构后, 波形缓冲区按 Protocol 节点、原始数据收集器按 Transport 节点分实例。
//! 本模块负责:
//! - 波形: 每源一个 WaveformWindowCache (引用计数, 供各波形 Tab 溯源订阅);
//!   另有"主波形源"驱动全局单例 waveformWindow (固定波形 Tab / 通道回退读取)
//! - 原始数据: 当前选中的 Transport 源驱动全局单例 rawDataBuffer

import { api } from '../tauri/tauri';
import { waveformWindow, rawDataBuffer, WaveformWindowCache } from './dataBuffer';
import { subscribeRawData } from './rawDataSubscription';

// ==================== 波形源 (source = Protocol 节点 id) ====================

interface WaveformSourceEntry {
  buffer: WaveformWindowCache;
  refs: number;
  cancel: () => void;
}

const waveformSources = new Map<string, WaveformSourceEntry>();

/// 获取 (引用计数 +1) 指定协议源的波形缓冲; 首次获取时建立订阅
export function acquireWaveformBuffer(sourceId: string): WaveformWindowCache {
  const existing = waveformSources.get(sourceId);
  if (existing) {
    existing.refs++;
    return existing.buffer;
  }
  const buffer = new WaveformWindowCache();
  const sub = api.subscribeWaveform(sourceId, (w) => buffer.set(w), {
    intervalMs: 33,
    maxPoints: 2000,
  });
  waveformSources.set(sourceId, { buffer, refs: 1, cancel: sub.cancel });
  return buffer;
}

/// 释放指定协议源的波形缓冲 (引用归零时取消订阅并丢弃缓冲)
export function releaseWaveformBuffer(sourceId: string): void {
  const entry = waveformSources.get(sourceId);
  if (!entry) return;
  entry.refs--;
  if (entry.refs <= 0) {
    entry.cancel();
    waveformSources.delete(sourceId);
  }
}

/// 只读查询 (不增加引用)
export function getWaveformBuffer(sourceId: string): WaveformWindowCache | null {
  return waveformSources.get(sourceId)?.buffer ?? null;
}

// ==================== 主波形源 (驱动全局单例 waveformWindow) ====================

let primaryWaveformSource: string | null = null;
let primaryWaveformSub: { cancel: () => void } | null = null;

/// 设置主波形源 (Protocol 节点 id); null = 无数据源 (清空并停止订阅)
export function setPrimaryWaveformSource(sourceId: string | null): void {
  if (sourceId === primaryWaveformSource) return;
  if (primaryWaveformSub) {
    primaryWaveformSub.cancel();
    primaryWaveformSub = null;
  }
  primaryWaveformSource = sourceId;
  waveformWindow.clear();
  if (sourceId) {
    primaryWaveformSub = api.subscribeWaveform(sourceId, (w) => waveformWindow.set(w), {
      intervalMs: 33,
      maxPoints: 2000,
    });
  }
}

export function getPrimaryWaveformSource(): string | null {
  return primaryWaveformSource;
}

// ==================== 原始数据源 (source = Transport 节点 id) ====================

let rawDataSource: string | null = null;
let rawDataSub: { cancel: () => void } | null = null;

/// 设置原始数据视图的字节源 (Transport 节点 id); null = 停止订阅
export function setRawDataSource(transportNodeId: string | null): void {
  if (transportNodeId === rawDataSource) return;
  if (rawDataSub) {
    rawDataSub.cancel();
    rawDataSub = null;
  }
  rawDataSource = transportNodeId;
  rawDataBuffer.clear();
  if (transportNodeId) {
    rawDataSub = subscribeRawData(transportNodeId, (batch) => rawDataBuffer.pushBatch(batch), {
      intervalMs: 100,
      maxBytes: 65536,
    });
  }
}

export function getRawDataSource(): string | null {
  return rawDataSource;
}

/// 清理全部源订阅 (应用卸载 / 事件监听重建时调用)
export function cleanupSourceManagers(): void {
  setPrimaryWaveformSource(null);
  setRawDataSource(null);
  for (const [id, entry] of waveformSources) {
    entry.cancel();
    waveformSources.delete(id);
  }
}
