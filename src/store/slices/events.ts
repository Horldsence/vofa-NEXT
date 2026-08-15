import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { rawDataBuffer } from '../../lib/buffers/dataBuffer';
import {
  subscribeGraphOutputs,
  subscribeCustomInputs,
  subscribeSpectrum,
} from '../../lib/buffers/graphSubscription';
import { canFrameBuffer } from '../../lib/buffers/canBuffer';
import { subscribeCanFrames } from '../../lib/buffers/canSubscription';
import { subscribeRawData } from '../../lib/buffers/rawDataSubscription';
import { logicSampleBuffer, decodedEventBuffer } from '../../lib/buffers/logicBuffer';
import { subscribeLogicSamples, subscribeDecodedEvents } from '../../lib/buffers/logicSubscription';
import type { ConnectionState, TransportStats } from '../../types';
import { cleanupWaveformSub, cleanupDetectedChannelsPoller } from './connection';

let unlistenFns: UnlistenFn[] = [];
let graphOutputSub: { cancel: () => void } | null = null;
let customInputSub: { cancel: () => void } | null = null;
let spectrumSub: { cancel: () => void } | null = null;
let canFramesSub: { cancel: () => void } | null = null;
let rawDataSub: { cancel: () => void } | null = null;
let logicSamplesSub: { cancel: () => void } | null = null;
let decodedEventsSub: { cancel: () => void } | null = null;

/// RAF 合批器: Channel 高频推送先写入模块级缓存,
/// 只在 RAF 回调中更新一次 zustand store (约 16ms 一次, 而非每条消息一次)。
/// 用于 graphOutputs / customInputs / spectrumResults 三条高频路径。
interface RafCoalescer<T> {
  push: (value: T) => void;
  cancel: () => void;
}
function makeRafCoalescer<T>(apply: (value: T) => void): RafCoalescer<T> {
  let pending: T | null = null;
  let rafId: number | null = null;
  return {
    push(value) {
      pending = value;
      if (rafId !== null) return;
      rafId = requestAnimationFrame(() => {
        rafId = null;
        const v = pending;
        pending = null;
        if (v !== null) apply(v);
      });
    },
    cancel() {
      if (rafId !== null) cancelAnimationFrame(rafId);
      rafId = null;
      pending = null;
    },
  };
}

export interface EventSlice {
  initEventListeners: () => Promise<() => void>;
}

export function createEventSlice(set: any, get: any): EventSlice {
  return {
    initEventListeners: async () => {
      unlistenFns.forEach((fn) => fn());
      unlistenFns = [];

      const unlistenState = await listen<ConnectionState>('transport:state', (event) => {
        set({ connectionState: event.payload });
      });

      const unlistenStats = await listen<TransportStats>('transport:rx', (event) => {
        set((s: any) => ({
          stats: {
            rx_bytes: s.stats.rx_bytes + event.payload.rx_bytes,
            tx_bytes: s.stats.tx_bytes + event.payload.tx_bytes,
            rx_frames: s.stats.rx_frames + event.payload.rx_frames,
            tx_frames: s.stats.tx_frames + event.payload.tx_frames,
            rx_dropped: event.payload.rx_dropped,
            rxDroppedWindow: event.payload.rx_dropped,
            rxDroppedTotal: s.stats.rxDroppedTotal + event.payload.rx_dropped,
          },
        }));
      });

      unlistenFns = [unlistenState, unlistenStats];

      const graphCoalescer = makeRafCoalescer<{ values: Record<string, Record<string, number>>; tick: number }>(
        (v) => set({ graphOutputs: v.values, graphOutputsTick: v.tick })
      );
      const customCoalescer = makeRafCoalescer<Record<string, Record<string, number>>>(
        (v) => set({ customInputs: v })
      );
      const spectrumCoalescer = makeRafCoalescer<Record<string, unknown>>(
        (v) => set({ spectrumResults: v })
      );

      if (graphOutputSub) graphOutputSub.cancel();
      graphOutputSub = subscribeGraphOutputs((snapshot) => {
        graphCoalescer.push({ values: snapshot.values, tick: snapshot.tick });
      });

      if (customInputSub) customInputSub.cancel();
      customInputSub = subscribeCustomInputs((batch) => {
        customCoalescer.push(batch.inputs);
      });

      if (spectrumSub) spectrumSub.cancel();
      spectrumSub = subscribeSpectrum((batch) => {
        spectrumCoalescer.push(batch.spectra);
      });

      if (canFramesSub) canFramesSub.cancel();
      canFramesSub = subscribeCanFrames((batch) => {
        canFrameBuffer.push(batch.frames);
      });

      if (rawDataSub) rawDataSub.cancel();
      rawDataSub = subscribeRawData((batch) => {
        rawDataBuffer.pushBatch(batch);
      }, { intervalMs: 100, maxBytes: 65536 });

      if (logicSamplesSub) logicSamplesSub.cancel();
      logicSamplesSub = subscribeLogicSamples((batch) => {
        logicSampleBuffer.push(batch.samples);
      });

      if (decodedEventsSub) decodedEventsSub.cancel();
      decodedEventsSub = subscribeDecodedEvents((batch) => {
        decodedEventBuffer.push(batch.events);
      });

      get().controlTabs.forEach((tab: any) => get().syncTabGraph(tab.id));

      return () => {
        unlistenFns.forEach((fn) => fn());
        unlistenFns = [];
        graphCoalescer.cancel();
        customCoalescer.cancel();
        spectrumCoalescer.cancel();
        cleanupWaveformSub();
        if (graphOutputSub) {
          graphOutputSub.cancel();
          graphOutputSub = null;
        }
        if (customInputSub) {
          customInputSub.cancel();
          customInputSub = null;
        }
        if (spectrumSub) {
          spectrumSub.cancel();
          spectrumSub = null;
        }
        if (canFramesSub) {
          canFramesSub.cancel();
          canFramesSub = null;
        }
        if (rawDataSub) {
          rawDataSub.cancel();
          rawDataSub = null;
        }
        if (logicSamplesSub) {
          logicSamplesSub.cancel();
          logicSamplesSub = null;
        }
        if (decodedEventsSub) {
          decodedEventsSub.cancel();
          decodedEventsSub = null;
        }
        cleanupDetectedChannelsPoller();
      };
    },
  };
}
