import { describe, expect, it, afterEach, beforeEach, vi } from 'vitest';
import { render, act } from '@testing-library/react';
import { useEffect, useState } from 'react';
import { tauriMock } from '../../test/setup';
import { canFrameBuffer } from '../buffers/canBuffer';
import { subscribeCanFrames } from '../buffers/canSubscription';
import { logicSampleBuffer, decodedEventBuffer } from '../buffers/logicBuffer';
import { rawDataBuffer, waveformWindow } from '../buffers/dataBuffer';
import type { CanFrame, CanFrameBatch, RawDataBatch, WaveformWindow } from '../../types';

/// 手工 rAF ticker — 覆盖全局 requestAnimationFrame, 让 buffer 的帧级节流完全确定
function installManualRaf() {
  const origRaf = globalThis.requestAnimationFrame;
  const origCaf = globalThis.cancelAnimationFrame;
  let queued: { id: number; cb: FrameRequestCallback }[] = [];
  let nextId = 1;
  globalThis.requestAnimationFrame = ((cb: FrameRequestCallback) => {
    const id = nextId++;
    queued.push({ id, cb });
    return id;
  });
  globalThis.cancelAnimationFrame = ((id: number) => {
    queued = queued.filter((q) => q.id !== id);
  });
  const flush = () => {
    const items = queued.splice(0);
    for (const { cb } of items) cb(0);
  };
  const flushAll = () => {
    while (queued.length > 0) flush();
  };
  return {
    flush,
    flushAll,
    pending: () => queued.length,
    restore: () => {
      globalThis.requestAnimationFrame = origRaf;
      globalThis.cancelAnimationFrame = origCaf;
    },
  };
}

function makeCanFrame(id: number): CanFrame {
  return {
    timestamp: id * 1000,
    id: id % 0x7ff,
    extended: false,
    rtr: false,
    dlc: 1,
    data: [id & 0xff],
    direction: 'Rx',
  };
}

/// 从 invoke mock 调用记录中取出某个命令注册的 Tauri Channel
function getChannelFor<T>(command: string): { onmessage: ((msg: T) => void) | null } {
  const kindByLegacyCommand: Record<string, string> = {
    subscribe_can_frames: 'can_frames',
  };
  const kind = kindByLegacyCommand[command];
  const calls = tauriMock.invoke.mock.calls as unknown as [
    string,
    { request?: { kind?: string }; onEvent?: { onmessage: ((msg: unknown) => void) | null } }
  ][];
  const call = calls.find((c) => c[0] === 'subscribe_data' && c[1].request?.kind === kind);
  const channel = call?.[1]?.onEvent;
  if (!channel) throw new Error(`channel not registered for ${command}`);
  return { onmessage: (payload) => channel.onmessage?.({ kind, payload }) };
}

let ticker: ReturnType<typeof installManualRaf>;

beforeEach(() => {
  ticker = installManualRaf();
  canFrameBuffer.clear();
  logicSampleBuffer.clear();
  decodedEventBuffer.clear();
  rawDataBuffer.clear();
  waveformWindow.clear();
  tauriMock.invoke.mockClear();
});

afterEach(() => {
  ticker.restore();
});

describe('buffer burst render-guard (RAF-throttled notify)', () => {
  it('canFrameBuffer: a burst of pushes within one frame causes exactly one component re-render', () => {
    const renderSpy = vi.fn();
    function Subscriber() {
      const [n, setN] = useState(0);
      useEffect(() => canFrameBuffer.subscribe(() => setN((c) => c + 1)), []);
      renderSpy();
      return <span data-testid="n">{n}</span>;
    }
    render(<Subscriber />);
    expect(renderSpy).toHaveBeenCalledTimes(1);

    // 同一帧内 push 30 次 — 不应有任何通知
    act(() => {
      for (let i = 0; i < 30; i++) canFrameBuffer.push([makeCanFrame(i)]);
    });
    expect(renderSpy).toHaveBeenCalledTimes(1);

    // 帧边界 flush → 恰好一次通知
    act(() => ticker.flush());
    expect(renderSpy).toHaveBeenCalledTimes(2);

    // 又一帧 burst → 又一次
    act(() => {
      for (let i = 0; i < 10; i++) canFrameBuffer.push([makeCanFrame(i)]);
    });
    act(() => ticker.flush());
    expect(renderSpy).toHaveBeenCalledTimes(3);
  });

  it('canFrameBuffer: burst delivered through the mocked Tauri event system is coalesced to one render per frame', () => {
    const renderSpy = vi.fn();
    function Subscriber() {
      const [n, setN] = useState(0);
      useEffect(() => canFrameBuffer.subscribe(() => setN((c) => c + 1)), []);
      renderSpy();
      return <span data-testid="n">{n}</span>;
    }
    render(<Subscriber />);

    // 与 events.ts 相同的接线: Channel onmessage → canFrameBuffer.push
    const sub = subscribeCanFrames((batch) => canFrameBuffer.push(batch.frames));
    const channel = getChannelFor<CanFrameBatch>('subscribe_can_frames');

    // 通过 Channel 一次性灌入 50 个批次
    act(() => {
      for (let i = 0; i < 50; i++) {
        channel.onmessage!({ seq: i, frames: [makeCanFrame(i)] });
      }
    });
    expect(renderSpy).toHaveBeenCalledTimes(1);

    act(() => ticker.flush());
    expect(renderSpy).toHaveBeenCalledTimes(2);
    sub.cancel();
  });

  it('logicSampleBuffer and decodedEventBuffer bursts are each coalesced to one render per frame', () => {
    const logicRender = vi.fn();
    function LogicSubscriber() {
      const [n, setN] = useState(0);
      useEffect(() => logicSampleBuffer.subscribe(() => setN((c) => c + 1)), []);
      logicRender();
      return <span>{n}</span>;
    }
    const eventRender = vi.fn();
    function EventSubscriber() {
      const [n, setN] = useState(0);
      useEffect(() => decodedEventBuffer.subscribe(() => setN((c) => c + 1)), []);
      eventRender();
      return <span>{n}</span>;
    }
    render(
      <>
        <LogicSubscriber />
        <EventSubscriber />
      </>
    );

    act(() => {
      for (let i = 0; i < 40; i++) {
        logicSampleBuffer.push([{ timestamp: i, channels: i % 2, channel_count: 8 }]);
        decodedEventBuffer.push([{ Uart: { timestamp: i, byte: i & 0xff, parity_ok: true } }]);
      }
    });
    expect(logicRender).toHaveBeenCalledTimes(1);
    expect(eventRender).toHaveBeenCalledTimes(1);

    act(() => ticker.flush());
    expect(logicRender).toHaveBeenCalledTimes(2);
    expect(eventRender).toHaveBeenCalledTimes(2);
  });

  it('rawDataBuffer: a burst of batches triggers one notification per frame', () => {
    const renderSpy = vi.fn();
    function Subscriber() {
      const [n, setN] = useState(0);
      useEffect(() => rawDataBuffer.subscribe(() => setN((c) => c + 1)), []);
      renderSpy();
      return <span>{n}</span>;
    }
    render(<Subscriber />);

    const makeBatch = (i: number): RawDataBatch => ({
      seq: i,
      chunks: [{ timestamp_us: i * 1000, bytes_b64: btoa(String.fromCharCode(i & 0xff)) }],
      total_bytes: 1,
      dropped_bytes: 0,
    });
    act(() => {
      for (let i = 0; i < 40; i++) rawDataBuffer.pushBatch(makeBatch(i));
    });
    expect(renderSpy).toHaveBeenCalledTimes(1);

    act(() => ticker.flush());
    expect(renderSpy).toHaveBeenCalledTimes(2);
  });

  it('waveformWindow stats: a burst of window sets notifies stats subscribers once per frame', () => {
    const renderSpy = vi.fn();
    function StatsSubscriber() {
      const [stats, setStats] = useState({ usage: 0, length: 0 });
      useEffect(
        () =>
          waveformWindow.subscribeStats((usage, length) => {
            setStats({ usage, length });
          }),
        []
      );
      renderSpy();
      return <span>{stats.length}</span>;
    }
    render(<StatsSubscriber />);
    // subscribeStats 在订阅时立即推一次当前值 → 挂载期多一次渲染
    expect(renderSpy).toHaveBeenCalledTimes(2);

    const makeWindow = (points: number): WaveformWindow => ({
      seq: points,
      timestamps: Array.from({ length: points }, (_, i) => i),
      channels: [Array.from({ length: points }, (_, i) => i)],
      channel_count: 1,
      buffer_points: points,
      buffer_capacity: 2000,
    });
    act(() => {
      for (let i = 0; i < 50; i++) waveformWindow.set(makeWindow(i + 1));
    });
    // 帧边界前统计通知被合并 — 无额外渲染
    expect(renderSpy).toHaveBeenCalledTimes(2);

    act(() => ticker.flush());
    expect(renderSpy).toHaveBeenCalledTimes(3);
  });
});
