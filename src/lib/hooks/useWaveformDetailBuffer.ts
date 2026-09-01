import { useEffect, useMemo, useRef, useState } from 'react';
import type { WaveformSeriesSelection } from '../../types';
import { WaveformWindowCache } from '../buffers/dataBuffer';
import { api } from '../tauri/tauri';

interface WaveformDetailOptions {
  sourceId: string | null;
  running: boolean;
  viewEndMs: number;
  viewSpanMs: number;
  pointBudget: number;
  intervalMs: number;
  selection: WaveformSeriesSelection;
  overviewBuffer: WaveformWindowCache;
}

export interface WaveformDetailState {
  detailBuffer: WaveformWindowCache;
  overviewBuffer: WaveformWindowCache;
  snapshotId: string | null;
  snapshotError: string | null;
}

const EMPTY_BUFFER = new WaveformWindowCache();

function createSourceBuffers(sourceId: string | null) {
  return {
    sourceId,
    liveDetail: new WaveformWindowCache(),
    stoppedDetail: new WaveformWindowCache(),
    stoppedOverview: new WaveformWindowCache(),
  };
}

/** 每个波形视图独占的按时基 detail 流及 Stop 原始快照生命周期。 */
export function useWaveformDetailBuffer({
  sourceId,
  running,
  viewEndMs,
  viewSpanMs,
  pointBudget,
  intervalMs,
  selection,
  overviewBuffer,
}: WaveformDetailOptions): WaveformDetailState {
  // 缓存身份与数据源绑定，换源时绝不展示上一来源的迟到快照。
  const sourceBuffers = useMemo(() => createSourceBuffers(sourceId), [sourceId]);
  const { liveDetail, stoppedDetail, stoppedOverview } = sourceBuffers;
  const [snapshotId, setSnapshotId] = useState<string | null>(null);
  const [snapshotError, setSnapshotError] = useState<string | null>(null);
  const generationRef = useRef(0);

  const selectionKey = useMemo(() => JSON.stringify(selection), [selection]);
  // eslint-disable-next-line react-hooks/exhaustive-deps -- JSON 内容相同即为同一后端选择规范
  const stableSelection = useMemo(() => selection, [selectionKey]);
  const overscanMs = viewSpanMs * 0.05;
  const requestStartMs = viewEndMs - viewSpanMs - overscanMs;
  const requestEndMs = viewEndMs + overscanMs;

  // Run: 75ms 防抖后建立当前视图独占订阅；generation 阻止迟到事件覆盖新窗口。
  useEffect(() => {
    if (!running || !sourceId) {
      if (!sourceId) liveDetail.clear();
      return;
    }
    const generation = ++generationRef.current;
    let cancel: (() => void) | null = null;
    const timer = window.setTimeout(() => {
      const subscription = api.subscribeWaveform(
        sourceId,
        (waveform) => {
          if (generation === generationRef.current) liveDetail.set(waveform);
        },
        {
          intervalMs,
          maxPoints: pointBudget,
          startMs: requestStartMs,
          endMs: requestEndMs,
          selection: stableSelection,
        },
      );
      cancel = subscription.cancel;
    }, 75);
    return () => {
      generationRef.current = generation + 1;
      window.clearTimeout(timer);
      cancel?.();
    };
  }, [
    sourceId,
    running,
    requestStartMs,
    requestEndMs,
    pointBudget,
    intervalMs,
    selectionKey,
    stableSelection,
    liveDetail,
  ]);

  // Stop: 立即冻结当前显示，再克隆后端原始缓存。Run/换源/卸载通过 cleanup 释放。
  useEffect(() => {
    if (running || !sourceId) {
      setSnapshotId(null);
      setSnapshotError(null);
      return;
    }
    stoppedDetail.set(liveDetail.get());
    stoppedOverview.set(overviewBuffer.get());
    setSnapshotId(null);
    setSnapshotError(null);
    let released = false;
    let createdId: string | null = null;
    void api.createWaveformSnapshot(sourceId)
      .then((created) => {
        createdId = created.snapshot_id;
        if (released) {
          void api.releaseWaveformSnapshot(created.snapshot_id);
          return;
        }
        stoppedOverview.set(created.overview);
        setSnapshotId(created.snapshot_id);
      })
      .catch((error: unknown) => {
        if (!released) setSnapshotError(String(error));
      });
    return () => {
      released = true;
      if (createdId) void api.releaseWaveformSnapshot(createdId);
    };
  }, [sourceId, running, liveDetail, overviewBuffer, stoppedDetail, stoppedOverview]);

  // Stop 下每次时基/位置/宽度/可见序列变化，都从原始快照重新生成 detail。
  useEffect(() => {
    if (running || !snapshotId) return;
    const generation = ++generationRef.current;
    const timer = window.setTimeout(() => {
      void api.queryWaveformSnapshot(
        snapshotId,
        requestStartMs,
        requestEndMs,
        pointBudget,
        stableSelection,
      ).then((waveform) => {
        if (generation === generationRef.current) stoppedDetail.set(waveform);
      }).catch((error: unknown) => {
        if (generation === generationRef.current) setSnapshotError(String(error));
      });
    }, 75);
    return () => {
      generationRef.current = generation + 1;
      window.clearTimeout(timer);
    };
  }, [
    running,
    snapshotId,
    requestStartMs,
    requestEndMs,
    pointBudget,
    selectionKey,
    stableSelection,
    stoppedDetail,
  ]);

  if (!sourceId) {
    return {
      detailBuffer: EMPTY_BUFFER,
      overviewBuffer: EMPTY_BUFFER,
      snapshotId: null,
      snapshotError: null,
    };
  }
  return {
    detailBuffer: running ? liveDetail : stoppedDetail,
    overviewBuffer: running ? overviewBuffer : stoppedOverview,
    snapshotId,
    snapshotError,
  };
}
