import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  subscribeGraphOutputs,
  subscribeCustomInputs,
  subscribeSpectrum,
  subscribeStringOutputs,
} from '../../lib/buffers/graphSubscription';
import { canFrameBuffer } from '../../lib/buffers/canBuffer';
import { subscribeCanFrames } from '../../lib/buffers/canSubscription';
import { logicSampleBuffer, decodedEventBuffer } from '../../lib/buffers/logicBuffer';
import { subscribeLogicSamples, subscribeDecodedEvents } from '../../lib/buffers/logicSubscription';
import {
  setPrimaryWaveformSource,
  getPrimaryWaveformSource,
  setRawDataSource,
  cleanupSourceManagers,
} from '../../lib/buffers/sourceManagers';
import { isGlobalNode } from '../appStoreHelpers';
import { useAppStore } from '../appStore';
import type { ConnectionState, TransportStats } from '../../types';
import { EMPTY_NODE_STATS, cleanupDetectedChannelsPollers } from './connection';

let unlistenFns: UnlistenFn[] = [];
let graphOutputSub: { cancel: () => void } | null = null;
let textOutputSub: { cancel: () => void } | null = null;
let customInputSub: { cancel: () => void } | null = null;
let spectrumSub: { cancel: () => void } | null = null;
let canFramesSub: { cancel: () => void } | null = null;
let logicSamplesSub: { cancel: () => void } | null = null;
let decodedEventsSub: { cancel: () => void } | null = null;
let storeUnsub: (() => void) | null = null;

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

/// transport:state payload 兼容解析 — 契约为 { node_id, state };
/// 后端过渡期可能仍发裸 ConnectionState (无节点信息, 忽略并告警)
function parseStateEvent(payload: unknown): { nodeId: string; state: ConnectionState } | null {
  if (payload && typeof payload === 'object' && 'node_id' in payload) {
    const p = payload as { node_id: string; state: ConnectionState };
    return { nodeId: p.node_id, state: p.state };
  }
  console.warn('[events] transport:state payload 缺少 node_id (后端契约未更新?):', payload);
  return null;
}

function parseRxEvent(payload: unknown): { nodeId: string; stats: TransportStats } | null {
  if (payload && typeof payload === 'object' && 'node_id' in payload) {
    const p = payload as { node_id: string; stats: TransportStats };
    return { nodeId: p.node_id, stats: p.stats };
  }
  console.warn('[events] transport:rx payload 缺少 node_id (后端契约未更新?):', payload);
  return null;
}

export interface EventSlice {
  initEventListeners: () => Promise<() => void>;
}

export function createEventSlice(set: any, get: any): EventSlice {
  /// 数据源对账: 主波形源 = 第一个 Protocol 节点; rawdata 源 = 选中或第一个 Transport 节点
  const reconcileSources = () => {
    const nodes: any[] = get().rfNodes;
    const firstProtocol = nodes.find((n) => n.type === 'protocol' && isGlobalNode(n));
    const primary = firstProtocol?.id ?? null;
    if (primary !== getPrimaryWaveformSource()) setPrimaryWaveformSource(primary);
    const selected: string | null = get().rawDataSourceNodeId;
    const transportIds = nodes.filter((n) => n.type === 'transport' && isGlobalNode(n)).map((n) => n.id);
    const effective =
      selected && transportIds.includes(selected) ? selected : (transportIds[0] ?? null);
    setRawDataSource(effective);
  };

  return {
    initEventListeners: async () => {
      unlistenFns.forEach((fn) => fn());
      unlistenFns = [];

      const unlistenState = await listen<unknown>('transport:state', (event) => {
        const parsed = parseStateEvent(event.payload);
        if (!parsed) return;
        set((s: any) => ({
          connectionStates: { ...s.connectionStates, [parsed.nodeId]: parsed.state },
        }));
      });

      const unlistenStats = await listen<unknown>('transport:rx', (event) => {
        const parsed = parseRxEvent(event.payload);
        if (!parsed) return;
        const st = parsed.stats;
        set((s: any) => {
          const prev = s.nodeStats[parsed.nodeId] ?? EMPTY_NODE_STATS;
          return {
            nodeStats: {
              ...s.nodeStats,
              [parsed.nodeId]: {
                rx_bytes: prev.rx_bytes + st.rx_bytes,
                tx_bytes: prev.tx_bytes + st.tx_bytes,
                rx_frames: prev.rx_frames + st.rx_frames,
                tx_frames: prev.tx_frames + st.tx_frames,
                rx_dropped: st.rx_dropped,
                rxDroppedWindow: st.rx_dropped,
                rxDroppedTotal: prev.rxDroppedTotal + st.rx_dropped,
              },
            },
          };
        });
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
      const textOutputCoalescer = makeRafCoalescer<Record<string, unknown>>(
        (v) => set({ customTextOutputs: v })
      );

      if (graphOutputSub) graphOutputSub.cancel();
      graphOutputSub = subscribeGraphOutputs((snapshot) => {
        graphCoalescer.push({ values: snapshot.values, tick: snapshot.tick });
      });

      if (customInputSub) customInputSub.cancel();
      customInputSub = subscribeCustomInputs((batch) => {
        customCoalescer.push(batch.inputs);
      });

      if (textOutputSub) textOutputSub.cancel();
      textOutputSub = subscribeStringOutputs((snapshot) => {
        textOutputCoalescer.push(snapshot.values);
      });

      if (spectrumSub) spectrumSub.cancel();
      spectrumSub = subscribeSpectrum((batch) => {
        spectrumCoalescer.push(batch.spectra);
      });

      if (canFramesSub) canFramesSub.cancel();
      canFramesSub = subscribeCanFrames((batch) => {
        canFrameBuffer.push(batch.frames);
      });

      if (logicSamplesSub) logicSamplesSub.cancel();
      logicSamplesSub = subscribeLogicSamples((batch) => {
        logicSampleBuffer.push(batch.samples);
      });

      if (decodedEventsSub) decodedEventsSub.cancel();
      decodedEventsSub = subscribeDecodedEvents((batch) => {
        decodedEventBuffer.push(batch.events);
      });

      // 数据源对账: 图节点 / 数据源选择变化时重订主波形与 rawdata 源
      reconcileSources();
      if (storeUnsub) storeUnsub();
      let prevNodes: unknown = null;
      let prevRawSource: unknown = undefined;
      storeUnsub = useAppStore.subscribe((s) => {
        if (s.rfNodes !== prevNodes || s.rawDataSourceNodeId !== prevRawSource) {
          prevNodes = s.rfNodes;
          prevRawSource = s.rawDataSourceNodeId;
          reconcileSources();
          get().ensureChannelsPolling?.();
        }
      });

      get().controlTabs.forEach((tab: any) => get().syncTabGraph(tab.id));

      return () => {
        unlistenFns.forEach((fn) => fn());
        unlistenFns = [];
        graphCoalescer.cancel();
        customCoalescer.cancel();
        spectrumCoalescer.cancel();
        cleanupSourceManagers();
        if (storeUnsub) {
          storeUnsub();
          storeUnsub = null;
        }
        if (graphOutputSub) {
          graphOutputSub.cancel();
          graphOutputSub = null;
        }
        if (customInputSub) {
          customInputSub.cancel();
          customInputSub = null;
        }
        if (textOutputSub) {
          textOutputSub.cancel();
          textOutputSub = null;
        }
        if (spectrumSub) {
          spectrumSub.cancel();
          spectrumSub = null;
        }
        if (canFramesSub) {
          canFramesSub.cancel();
          canFramesSub = null;
        }
        if (logicSamplesSub) {
          logicSamplesSub.cancel();
          logicSamplesSub = null;
        }
        if (decodedEventsSub) {
          decodedEventsSub.cancel();
          decodedEventsSub = null;
        }
        cleanupDetectedChannelsPollers();
      };
    },
  };
}
