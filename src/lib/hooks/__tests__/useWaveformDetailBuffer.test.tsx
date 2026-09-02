import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { WaveformWindow } from '../../../types';
import { WaveformWindowCache } from '../../buffers/dataBuffer';
import { api } from '../../tauri/tauri';
import { useWaveformDetailBuffer } from '../useWaveformDetailBuffer';

const selection = { channels: [0], derived: [] };

function waveform(value: number, latestTimestampUs = 10_000): WaveformWindow {
  return {
    seq: 0,
    timestamps: Float64Array.from([-0.001, 0]),
    channels: [Float32Array.from([value, value])],
    channel_count: 1,
    derived: {},
    buffer_points: 2,
    buffer_capacity: 100,
    latest_timestamp_us: latestTimestampUs,
    raw_window_points: 2,
    sampling: 'raw',
  };
}

describe('useWaveformDetailBuffer', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('为同源视图建立独立窗口，并拒绝旧 generation 的迟到事件', async () => {
    const callbacks: ((value: WaveformWindow) => void)[] = [];
    const cancellations: ReturnType<typeof vi.fn>[] = [];
    const subscribe = vi.spyOn(api, 'subscribeWaveform').mockImplementation((_, callback) => {
      callbacks.push(callback);
      const cancel = vi.fn();
      cancellations.push(cancel);
      return { cancel };
    });
    const overview = new WaveformWindowCache();

    const first = renderHook(
      ({ endMs }) => useWaveformDetailBuffer({
        sourceId: 'source',
        running: true,
        viewEndMs: endMs,
        viewSpanMs: 100,
        pointBudget: 2_000,
        intervalMs: 20,
        selection,
        overviewBuffer: overview,
      }),
      { initialProps: { endMs: 0 } },
    );
    const second = renderHook(() => useWaveformDetailBuffer({
      sourceId: 'source',
      running: true,
      viewEndMs: -500,
      viewSpanMs: 1_000,
      pointBudget: 4_000,
      intervalMs: 20,
      selection,
      overviewBuffer: overview,
    }));

    await act(() => vi.advanceTimersByTime(75));
    expect(subscribe).toHaveBeenNthCalledWith(
      1,
      'source',
      expect.any(Function),
      expect.objectContaining({ startMs: -105, endMs: 5, maxPoints: 2_000 }),
    );
    expect(subscribe).toHaveBeenNthCalledWith(
      2,
      'source',
      expect.any(Function),
      expect.objectContaining({ startMs: -1_550, endMs: -450, maxPoints: 4_000 }),
    );

    act(() => callbacks[0](waveform(1)));
    expect(first.result.current.detailBuffer.get().channels[0][0]).toBe(1);

    first.rerender({ endMs: -100 });
    expect(cancellations[0]).toHaveBeenCalledOnce();
    act(() => callbacks[0](waveform(99)));
    expect(first.result.current.detailBuffer.get().channels[0][0]).toBe(1);

    await act(() => vi.advanceTimersByTime(75));
    expect(subscribe).toHaveBeenLastCalledWith(
      'source',
      expect.any(Function),
      expect.objectContaining({ startMs: -205, endMs: -95 }),
    );

    first.unmount();
    second.unmount();
  });

  it('Stop 时冻结画面、查询原始快照，并在恢复 Run 时释放', async () => {
    let liveCallback: ((value: WaveformWindow) => void) | null = null;
    vi.spyOn(api, 'subscribeWaveform').mockImplementation((_, callback) => {
      liveCallback = callback;
      return { cancel: vi.fn() };
    });
    const snapshotOverview = waveform(2, 20_000);
    const snapshotDetail = waveform(3, 20_000);
    vi.spyOn(api, 'createWaveformSnapshot').mockResolvedValue({
      snapshot_id: 'snapshot-1',
      overview: snapshotOverview,
    });
    const query = vi.spyOn(api, 'queryWaveformSnapshot').mockResolvedValue(snapshotDetail);
    const release = vi.spyOn(api, 'releaseWaveformSnapshot').mockResolvedValue(undefined);
    const overview = new WaveformWindowCache();
    overview.set(waveform(5));

    const { result, rerender, unmount } = renderHook(
      ({ running, endMs }) => useWaveformDetailBuffer({
        sourceId: 'source',
        running,
        viewEndMs: endMs,
        viewSpanMs: 100,
        pointBudget: 2_000,
        intervalMs: 20,
        selection,
        overviewBuffer: overview,
      }),
      { initialProps: { running: true, endMs: 0 } },
    );

    await act(() => vi.advanceTimersByTime(75));
    act(() => liveCallback?.(waveform(1)));
    rerender({ running: false, endMs: 0 });
    expect(result.current.detailBuffer.get().channels[0][0]).toBe(1);

    await act(async () => Promise.resolve());
    expect(result.current.snapshotId).toBe('snapshot-1');
    expect(result.current.overviewBuffer.get()).toBe(snapshotOverview);
    await act(() => vi.advanceTimersByTime(75));
    await act(async () => Promise.resolve());
    expect(query).toHaveBeenCalledWith('snapshot-1', -105, 5, 2_000, selection);
    expect(result.current.detailBuffer.get()).toBe(snapshotDetail);

    rerender({ running: true, endMs: 0 });
    expect(release).toHaveBeenCalledWith('snapshot-1');
    unmount();
  });

  it('切换数据源时立即换用空缓存，并丢弃旧来源迟到事件', async () => {
    const callbacks: ((value: WaveformWindow) => void)[] = [];
    vi.spyOn(api, 'subscribeWaveform').mockImplementation((_, callback) => {
      callbacks.push(callback);
      return { cancel: vi.fn() };
    });
    const overview = new WaveformWindowCache();
    const { result, rerender } = renderHook(
      ({ sourceId }) => useWaveformDetailBuffer({
        sourceId,
        running: true,
        viewEndMs: 0,
        viewSpanMs: 100,
        pointBudget: 2_000,
        intervalMs: 20,
        selection,
        overviewBuffer: overview,
      }),
      { initialProps: { sourceId: 'source-a' } },
    );

    await act(() => vi.advanceTimersByTime(75));
    act(() => callbacks[0](waveform(1)));
    const sourceABuffer = result.current.detailBuffer;
    expect(sourceABuffer.get().channels[0][0]).toBe(1);

    rerender({ sourceId: 'source-b' });
    expect(result.current.detailBuffer).not.toBe(sourceABuffer);
    expect(result.current.detailBuffer.get().timestamps.length).toBe(0);
    act(() => callbacks[0](waveform(99)));
    expect(result.current.detailBuffer.get().timestamps.length).toBe(0);

    await act(() => vi.advanceTimersByTime(75));
    act(() => callbacks[1](waveform(2)));
    expect(result.current.detailBuffer.get().channels[0][0]).toBe(2);
  });
});
