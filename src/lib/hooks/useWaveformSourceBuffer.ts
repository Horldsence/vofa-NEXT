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
  const [buffer, setBuffer] = useState<WaveformWindowCache>(() => {
    if (sourceId === null) return EMPTY_BUFFER;
    if (sourceId === getPrimaryWaveformSource()) return waveformWindow;
    return acquireWaveformBuffer(sourceId);
  });

  useEffect(() => {
    if (sourceId === null) {
      setBuffer(EMPTY_BUFFER);
      return;
    }
    if (sourceId === getPrimaryWaveformSource()) {
      setBuffer(waveformWindow);
      return;
    }
    const b = acquireWaveformBuffer(sourceId);
    setBuffer(b);
    return () => releaseWaveformBuffer(sourceId);
  }, [sourceId]);

  return buffer;
}
