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
  cancel: (() => void) | null;
  /// 波形停止时暂停推送 (保留缓存与引用计数, 恢复时按原参数重订阅)
  active: boolean;
}

const waveformSources = new Map<string, WaveformSourceEntry>();

/// 概览流推送间隔 — 缩略图/概览 10fps 足够;
/// 后端按金字塔预算层生成 min-max；频率仍会影响快照拷贝与 IPC 开销
const OVERVIEW_INTERVAL_MS = 100;
const OVERVIEW_MAX_POINTS = 2000;

function subscribeOverview(
  sourceId: string,
  buffer: WaveformWindowCache,
): { cancel: () => void } {
  return api.subscribeWaveform(sourceId, (w) => buffer.set(w), {
    intervalMs: OVERVIEW_INTERVAL_MS,
    maxPoints: OVERVIEW_MAX_POINTS,
  });
}

/// 获取 (引用计数 +1) 指定协议源的波形缓冲; 首次获取时建立订阅
export function acquireWaveformBuffer(sourceId: string): WaveformWindowCache {
  const existing = waveformSources.get(sourceId);
  if (existing) {
    existing.refs++;
    return existing.buffer;
  }
  const buffer = new WaveformWindowCache();
  const sub = subscribeOverview(sourceId, buffer);
  waveformSources.set(sourceId, {
    buffer,
    refs: 1,
    cancel: sub.cancel,
    active: true,
  });
  return buffer;
}

/// 释放指定协议源的波形缓冲 (引用归零时取消订阅并丢弃缓冲)
export function releaseWaveformBuffer(sourceId: string): void {
  const entry = waveformSources.get(sourceId);
  if (!entry) return;
  entry.refs--;
  if (entry.refs <= 0) {
    entry.cancel?.();
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
/// 主波形源推送是否被暂停 (波形停止时冻结, 恢复时重订阅)
let primaryWaveformActive = true;

/// 设置主波形源 (Protocol 节点 id); null = 无数据源 (清空并停止订阅)
export function setPrimaryWaveformSource(sourceId: string | null): void {
  if (sourceId === primaryWaveformSource) return;
  if (primaryWaveformSub) {
    primaryWaveformSub.cancel();
    primaryWaveformSub = null;
  }
  primaryWaveformSource = sourceId;
  waveformWindow.clear();
  if (sourceId && primaryWaveformActive) {
    primaryWaveformSub = subscribeOverview(sourceId, waveformWindow);
  }
}

export function getPrimaryWaveformSource(): string | null {
  return primaryWaveformSource;
}

/// 暂停/恢复某源的概览推送 — 波形停止时后端不再空转全缓冲 min-max;
/// 缓存条目与引用计数保留 (对象身份不变, 下游无 effect 抖动), 恢复时按原参数重订阅
export function setWaveformOverviewActive(sourceId: string, active: boolean): void {
  const entry = waveformSources.get(sourceId);
  if (entry && entry.active !== active) {
    entry.active = active;
    if (active) {
      const sub = subscribeOverview(sourceId, entry.buffer);
      entry.cancel = sub.cancel;
    } else {
      entry.cancel?.();
      entry.cancel = null;
    }
  }
  if (sourceId === primaryWaveformSource && primaryWaveformActive !== active) {
    primaryWaveformActive = active;
    if (active) {
      primaryWaveformSub = subscribeOverview(sourceId, waveformWindow);
    } else {
      primaryWaveformSub?.cancel();
      primaryWaveformSub = null;
    }
  }
}

/// 清理全部源订阅 (应用卸载 / 事件监听重建时调用)
export function cleanupSourceManagers(): void {
  setPrimaryWaveformSource(null);
  primaryWaveformActive = true;
  for (const [id, entry] of waveformSources) {
    entry.cancel?.();
    waveformSources.delete(id);
  }
}
