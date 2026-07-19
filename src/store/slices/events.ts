import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { rawDataBuffer } from '../../lib/dataBuffer';
import {
  subscribeGraphOutputs,
  subscribeCustomInputs,
  subscribeSpectrum,
} from '../../lib/graphSubscription';
import { canFrameBuffer } from '../../lib/canBuffer';
import { subscribeCanFrames } from '../../lib/canSubscription';
import { subscribeRawData } from '../../lib/rawDataSubscription';
import { logicSampleBuffer, decodedEventBuffer } from '../../lib/logicBuffer';
import { subscribeLogicSamples, subscribeDecodedEvents } from '../../lib/logicSubscription';
import type { DataFrame, ConnectionState, TransportStats, CanFrame, LogicSample, DecodedEvent } from '../../types';
import { cleanupWaveformSub, cleanupDetectedChannelsPoller } from './connection';

let unlistenFns: UnlistenFn[] = [];
let graphOutputSub: { cancel: () => void } | null = null;
let customInputSub: { cancel: () => void } | null = null;
let spectrumSub: { cancel: () => void } | null = null;
let canFramesSub: { cancel: () => void } | null = null;
let rawDataSub: { cancel: () => void } | null = null;
let logicSamplesSub: { cancel: () => void } | null = null;
let decodedEventsSub: { cancel: () => void } | null = null;

export interface EventSlice {
  initEventListeners: () => Promise<() => void>;
}

export function createEventSlice(set: any, get: any): EventSlice {
  return {
    initEventListeners: async () => {
      unlistenFns.forEach((fn) => fn());
      unlistenFns = [];

      const unlistenFrames = await listen<DataFrame[]>('transport:frames', () => {
        // 后端已将帧推入 DataBuffer, 通过 subscribe_waveform Channel 推送窗口
      });

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
          },
        }));
      });

      const unlistenCanFrames = await listen<{ frames: CanFrame[] }>('transport:can-frames', () => {
        // no-op: buffer 已由 Channel 路径维护
      });

      const unlistenLogic = await listen<{ samples: LogicSample[] }>('transport:logic-samples', () => {
        // no-op: buffer 已由 Channel 路径维护
      });

      const unlistenDecoded = await listen<{ events: DecodedEvent[] }>('transport:decoded-events', () => {
        // no-op: buffer 已由 Channel 路径维护
      });

      unlistenFns = [unlistenFrames, unlistenState, unlistenStats, unlistenCanFrames, unlistenLogic, unlistenDecoded];

      if (graphOutputSub) graphOutputSub.cancel();
      graphOutputSub = subscribeGraphOutputs((snapshot) => {
        set({
          graphOutputs: snapshot.values,
          graphOutputsTick: snapshot.tick,
        });
      });

      if (customInputSub) customInputSub.cancel();
      customInputSub = subscribeCustomInputs((batch) => {
        set({ customInputs: batch.inputs });
      });

      if (spectrumSub) spectrumSub.cancel();
      spectrumSub = subscribeSpectrum((batch) => {
        set({ spectrumResults: batch.spectra });
      });

      if (canFramesSub) canFramesSub.cancel();
      canFramesSub = subscribeCanFrames((batch) => {
        canFrameBuffer.push(batch.frames);
      });

      if (rawDataSub) rawDataSub.cancel();
      rawDataSub = subscribeRawData((batch) => {
        rawDataBuffer.pushBatch(batch);
      }, { intervalMs: 50, maxBytes: 65536 });

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
