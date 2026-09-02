//! 按源 (节点 id) 管理波形 / 原始数据订阅与前端缓冲
//!
//! 后端重构后, 波形缓冲区按 Protocol 节点、原始数据收集器按 Transport 节点分实例。
//! 本模块负责:
//! - 波形: 每源一个 WaveformWindowCache (引用计数, 供各波形 Tab 溯源订阅);
//!   另有"主波形源"驱动全局单例 waveformWindow (固定波形 Tab / 通道回退读取)
//! RawData 由实际打开的视图按源引用计数订阅，避免状态栏常驻隐藏字节流。

import { api } from '../tauri/tauri';
import { waveformWindow, WaveformWindowCache } from './dataBuffer';

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

/// 反查缓冲对应的协议源 id (波形图按 buffer 引用溯源订阅时使用);
/// 主波形单例 (无独立源缓冲) 返回主源 id
export function waveformSourceIdOf(buffer: WaveformWindowCache): string | null {
  if (buffer === waveformWindow) return primaryWaveformSource;
  for (const [id, entry] of waveformSources) {
    if (entry.buffer === buffer) return id;
  }
  return null;
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

/// 清理全部源订阅 (应用卸载 / 事件监听重建时调用)
export function cleanupSourceManagers(): void {
  setPrimaryWaveformSource(null);
  for (const [id, entry] of waveformSources) {
    entry.cancel();
    waveformSources.delete(id);
  }
}
