import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import { act } from '@testing-library/react';
import { tauriMock } from '../../test/setup';
import { useAppStore } from '../appStore';
import type { GraphOutputSnapshot, CustomInputBatch, SpectrumBatch } from '../../lib/buffers/graphSubscription';
import type { CanFrameBatch } from '../../types';
import type { GraphStateSlice } from '../slices/graphState';
import { canFrameBuffer } from '../../lib/buffers/canBuffer';

/// 手工 rAF ticker — 覆盖全局 requestAnimationFrame, 让 events.ts 的帧级节流确定
function installManualRaf() {
  const origRaf = globalThis.requestAnimationFrame;
  const origCaf = globalThis.cancelAnimationFrame;
  let queued: { id: number; cb: FrameRequestCallback }[] = [];
  let nextId = 1;
  globalThis.requestAnimationFrame = ((cb: FrameRequestCallback) => {
    const id = nextId++;
    queued.push({ id, cb });
    return id;
  }) as typeof requestAnimationFrame;
  globalThis.cancelAnimationFrame = ((id: number) => {
    queued = queued.filter((q) => q.id !== id);
  }) as typeof cancelAnimationFrame;
  const flush = () => {
    const items = queued.splice(0);
    for (const { cb } of items) cb(0);
  };
  return {
    flush,
    pending: () => queued.length,
    restore: () => {
      globalThis.requestAnimationFrame = origRaf;
      globalThis.cancelAnimationFrame = origCaf;
    },
  };
}

function getChannelFor<T>(command: string): { onmessage: ((msg: T) => void) | null } {
  const kindByLegacyCommand: Record<string, string> = {
    subscribe_graph_outputs: 'graph_outputs',
    subscribe_custom_inputs: 'custom_inputs',
    subscribe_spectrum: 'spectrum',
    subscribe_can_frames: 'can_frames',
  };
  const kind = kindByLegacyCommand[command];
  const calls = tauriMock.invoke.mock.calls as unknown as [
    string,
    { request?: { kind?: string }; onEvent?: { onmessage: ((msg: unknown) => void) | null } }
  ][];
  const call = calls.find((c) => c[0] === 'subscribe_display' && c[1].request?.kind === kind);
  const channel = call?.[1]?.onEvent;
  if (!channel) throw new Error(`channel not registered for ${command}`);
  return {
    onmessage: (payload) =>
      channel.onmessage?.({
        kind,
        payload: kind === 'spectrum' ? (payload as SpectrumBatch).spectra : payload,
      }),
  };
}

let ticker: ReturnType<typeof installManualRaf>;
let cleanup: () => void;

beforeEach(async () => {
  ticker = installManualRaf();
  cleanup = await useAppStore.getState().initEventListeners();
});

afterEach(() => {
  cleanup();
  ticker.restore();
  tauriMock.invoke.mockClear();
});

describe('events.ts graphOutputs batching', () => {
  it('coalesces a burst of graph output snapshots into one store update per frame', () => {
    const channel = getChannelFor<GraphOutputSnapshot>('subscribe_graph_outputs');

    const storeTickChanges: number[] = [];
    useAppStore.subscribe((state, prev) => {
      if (state.graphOutputsTick !== prev.graphOutputsTick) {
        storeTickChanges.push(state.graphOutputsTick);
      }
    });

    // 同一帧内灌入 100 个快照
    act(() => {
      for (let i = 0; i < 100; i++) {
        const snapshot: GraphOutputSnapshot = { tick: i + 1, values: { widget1: { out: i } } };
        channel.onmessage!(snapshot);
      }
    });

    // 帧边界前不写 store
    expect(useAppStore.getState().graphOutputsTick).toBe(0);
    expect(storeTickChanges).toHaveLength(0);

    act(() => ticker.flush());

    // 帧边界: 恰好一次写入, 值为最新快照
    expect(useAppStore.getState().graphOutputsTick).toBe(100);
    expect(useAppStore.getState().graphOutputs).toEqual({ widget1: { out: 99 } });
    expect(storeTickChanges).toEqual([100]);
  });

  it('still applies the latest snapshot when bursts arrive across multiple frames', () => {
    const channel = getChannelFor<GraphOutputSnapshot>('subscribe_graph_outputs');

    const pushBurst = (offset: number) => {
      act(() => {
        for (let i = 0; i < 10; i++) {
          const snapshot: GraphOutputSnapshot = { tick: offset + i, values: { w: { a: offset + i } } };
          channel.onmessage!(snapshot);
        }
      });
      act(() => ticker.flush());
    };

    pushBurst(1);
    pushBurst(101);
    pushBurst(201);

    expect(useAppStore.getState().graphOutputsTick).toBe(210);
    expect(useAppStore.getState().graphOutputs).toEqual({ w: { a: 210 } });
  });
});

describe('events.ts payload contract', () => {
  it('graphOutputs/customInputs/spectrum payloads keep their documented shapes in the store', () => {
    // graphOutputs 已在上方经真实 Channel 快照验证; 此处验证 customInputs / spectrum / can 批次
    const customChannel = getChannelFor<CustomInputBatch>('subscribe_custom_inputs');
    const spectrumChannel = getChannelFor<SpectrumBatch>('subscribe_spectrum');
    const canChannel = getChannelFor<CanFrameBatch>('subscribe_can_frames');

    const customBatch: CustomInputBatch = { inputs: { cw1: { knob: 42 } } };
    act(() => customChannel.onmessage!(customBatch));
    act(() => ticker.flush()); // customInputs 已改为 RAF 合批
    expect(useAppStore.getState().customInputs).toEqual({ cw1: { knob: 42 } });

    const spectrumBatch: SpectrumBatch = {
      spectra: {
        sink1: { frequencies: [0, 1], values: [0.1, 0.2] },
      },
    };
    act(() => spectrumChannel.onmessage!(spectrumBatch));
    act(() => ticker.flush()); // spectrumResults 已改为 RAF 合批
    expect(useAppStore.getState().spectrumResults).toEqual(spectrumBatch.spectra);

    // CAN 批次进入 canFrameBuffer (RAF 节流), 帧形状原样保留
    const canBatch: CanFrameBatch = {
      seq: 0,
      frames: [{ timestamp: 5, id: 0x123, extended: false, rtr: false, dlc: 1, data: [0xaa], direction: 'Rx' }],
    };
    act(() => canChannel.onmessage!(canBatch));
    act(() => ticker.flush());
    expect(canFrameBuffer.getRecent(1)).toEqual(canBatch.frames);
  });

  it('store slice field types are assignable from the subscription payload types (compile-time contract)', () => {
    // 以下赋值在编译期验证: 事件负载结构 = store 切片结构 (不改契约)
    const values: GraphOutputSnapshot['values'] = {} as GraphStateSlice['graphOutputs'];
    const tick: GraphOutputSnapshot['tick'] = 0 as GraphStateSlice['graphOutputsTick'];
    const inputs: CustomInputBatch['inputs'] = {} as GraphStateSlice['customInputs'];
    const spectra: SpectrumBatch['spectra'] = {} as GraphStateSlice['spectrumResults'];
    expect(values).toBeDefined();
    expect(tick).toBe(0);
    expect(inputs).toBeDefined();
    expect(spectra).toBeDefined();
  });
});
