//! 波形数据源 buffer hook — 按 Protocol 源节点获取对应 WaveformWindowCache
//!
//! - sourceId === null (无连接/溯源不到): 返回共享空缓冲 (图表显示空态, 不订阅)
//! - sourceId === 主波形源: 返回全局单例 waveformWindow (固定 Tab / 通道回退共用)
//! - 其他: 从注册表按引用计数 acquire/release (每源一个订阅)
import { useEffect, useState } from 'react';
import { waveformWindow, WaveformWindowCache } from '../buffers/dataBuffer';
import {
  acquireWaveformBuffer,
  releaseWaveformBuffer,
  getPrimaryWaveformSource,
} from '../buffers/sourceManagers';

/// 共享空缓冲 — 无数据源时的占位 (永不订阅)
const EMPTY_BUFFER = new WaveformWindowCache();

export function useWaveformSourceBuffer(sourceId: string | null): WaveformWindowCache {
  const [resolved, setResolved] = useState<{
    sourceId: string | null;
    buffer: WaveformWindowCache;
  }>(() => ({
    sourceId,
    buffer: sourceId !== null && sourceId === getPrimaryWaveformSource()
      ? waveformWindow
      : EMPTY_BUFFER,
  }));

  useEffect(() => {
    if (sourceId === null) {
      setResolved({ sourceId, buffer: EMPTY_BUFFER });
      return;
    }
    if (sourceId === getPrimaryWaveformSource()) {
      setResolved({ sourceId, buffer: waveformWindow });
      return;
    }
    const b = acquireWaveformBuffer(sourceId);
    setResolved({ sourceId, buffer: b });
    return () => releaseWaveformBuffer(sourceId);
  }, [sourceId]);

  // effect 运行前先返回空缓存，避免换源后的首帧继续显示上一来源概览。
  return resolved.sourceId === sourceId ? resolved.buffer : EMPTY_BUFFER;
}
