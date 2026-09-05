import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { RunState } from '../../types';
import {
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
  cleanupSourceManagers,
} from '../../lib/buffers/sourceManagers';
import { isGlobalNode, adoptSourceGraph, isSyncInFlight, hydrateWorkspaceFromBackend, syncWorkspaceMeta } from '../appStoreHelpers';
import { useAppStore } from '../appStore';
import type { GraphSourceEventPayload } from '../../lib/tauri/tauri';
import type { ConnectionState, TransportStats } from '../../types';
import { EMPTY_NODE_STATS } from './connection';
import type { AppSlice } from './types';
import type { GraphStateSlice } from './graphState';
import type { GraphDerivedPayload } from './derived';
import type { GraphCompileEvent } from './compileStatus';

let unlistenFns: UnlistenFn[] = [];
let textOutputSub: { cancel: () => void } | null = null;
let spectrumSub: { cancel: () => void } | null = null;
let canFramesSub: { cancel: () => void } | null = null;
let logicSamplesSub: { cancel: () => void } | null = null;
let decodedEventsSub: { cancel: () => void } | null = null;
let storeUnsub: (() => void) | null = null;
let metaStoreUnsub: (() => void) | null = null;

/// RAF 合批器: Channel 高频推送先写入模块级缓存,
/// 只在 RAF 回调中更新一次 zustand store (约 16ms 一次, 而非每条消息一次)。
/// 用于字符串/频谱等仍采用 latest-value JSON 快照的路径。
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

/// protocol:channels-detected payload 兼容解析 — 契约为 { node_id, channels };
/// 后端由 feed_protocol 在检测值变化时 (None→Some(n) 或 Some(a)→Some(b)) 主动推送
function parseChannelsDetectedEvent(
  payload: unknown
): { nodeId: string; channels: number } | null {
  if (
    payload &&
    typeof payload === 'object' &&
    'node_id' in payload &&
    'channels' in payload &&
    typeof (payload as { channels: unknown }).channels === 'number'
  ) {
    const p = payload as { node_id: string; channels: number };
    return { nodeId: p.node_id, channels: p.channels };
  }
  console.warn('[events] protocol:channels-detected payload 契约不符:', payload);
  return null;
}

/// graph:derived payload 解析 — 后端 update_tab_graph/remove_tab_graph emit;
/// 契约为 `{ nodes: [{ node_id, ports: [{ name, domain }], effective_channels? }] }`。
/// 与 store/slices/derived.ts 的 GraphDerivedPayload 同步 (单一契约, 双向核对)。
function parseGraphDerivedEvent(payload: unknown): GraphDerivedPayload | null {
  if (
    payload &&
    typeof payload === 'object' &&
    'nodes' in payload &&
    Array.isArray((payload).nodes)
  ) {
    return payload as GraphDerivedPayload;
  }
  console.warn('[events] graph:derived payload 契约不符:', payload);
  return null;
}

/// graph:compile payload 解析 — 编译队列对外广播;
/// 契约为 `{ tab_id, state, queued_seq, report }` (state: pending|compiling|ok|error).
function parseGraphCompileEvent(
  payload: unknown,
): GraphCompileEvent | null {
  if (
    payload &&
    typeof payload === 'object' &&
    'tab_id' in payload &&
    'state' in payload &&
    'queued_seq' in payload
  ) {
    return payload as GraphCompileEvent;
  }
  console.warn('[events] graph:compile payload 契约不符:', payload);
  return null;
}

/// graph:source payload 解析 — tab 权威源图 (前端画布收敛依据)。
/// 契约为 `{ tab_id, version, nodes: NodeDef[], edges: Edge[] }` (snake_case)。
function parseGraphSourceEvent(payload: unknown): GraphSourceEventPayload | null {
  if (
    payload &&
    typeof payload === 'object' &&
    'tab_id' in payload &&
    'version' in payload &&
    'nodes' in payload &&
    'edges' in payload &&
    Array.isArray((payload as { edges: unknown }).edges)
  ) {
    return payload as GraphSourceEventPayload;
  }
  console.warn('[events] graph:source payload 契约不符:', payload);
  return null;
}

export interface EventSlice {
  initEventListeners: () => Promise<() => void>;
}

export const createEventSlice: AppSlice<EventSlice> = (set, get) => {
  /// 数据源对账: 主波形源 = 第一个 Protocol 节点。RawData 由可见视图自行订阅。
  const reconcileSources = () => {
    const nodes = get().rfNodes;
    const firstProtocol = nodes.find((n) => n.type === 'protocol' && isGlobalNode(n));
    const primary = firstProtocol?.id ?? null;
    if (primary !== getPrimaryWaveformSource()) setPrimaryWaveformSource(primary);
  };

  return {
    initEventListeners: async () => {
      unlistenFns.forEach((fn) => fn());
      unlistenFns = [];

      const unlistenState = await listen<unknown>('transport:state', (event) => {
        const parsed = parseStateEvent(event.payload);
        if (!parsed) return;
        set((s) => ({
          connectionStates: { ...s.connectionStates, [parsed.nodeId]: parsed.state },
        }));
      });

      const unlistenStats = await listen<unknown>('transport:rx', (event) => {
        const parsed = parseRxEvent(event.payload);
        if (!parsed) return;
        const st = parsed.stats;
        set((s) => {
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

      const unlistenChannels = await listen<unknown>('protocol:channels-detected', (event) => {
        const parsed = parseChannelsDetectedEvent(event.payload);
        if (!parsed) return;
        const { nodeId, channels } = parsed;
        // 写 detectedChannels (后端单一权威; UI 派生端口表 / 通道数随之刷新)
        // 命中节点 → 计算 effective, 更新节点 data.channels 并重同步全 tab 图
        // (ProtocolSource 的 ch 端口数随之变化)
        set((s) => {
          const node = s.rfNodes.find((n) => n.id === nodeId && n.type === 'protocol');
          if (!node) {
            // 节点已删 (前端未同步移除) — 清理孤儿 key 避免后续误用
            if (nodeId in s.detectedChannels) {
              const { [nodeId]: _drop, ...rest } = s.detectedChannels;
              return { detectedChannels: rest };
            }
            return {};
          }
          const config = (node.data as { config: { channels?: number | null } }).config;
          const manual = config?.channels ?? null;
          const effective = manual ?? channels;
          const rfNodes = s.rfNodes.map((n) =>
            n.id === nodeId ? { ...n, data: { ...n.data, channels: effective } } : n
          );
          return { detectedChannels: { ...s.detectedChannels, [nodeId]: channels }, rfNodes };
        });
        get().controlTabs.forEach((tab) => { void get().syncTabGraph(tab.id); });
      });

      // graph:derived — 后端 update_tab_graph / remove_tab_graph 提交后 emit;
      // 写 derivedPorts (端口表渲染单一权威); 与 update_tab_graph 响应合并写入,
      // 后到的事件作为权威覆盖前者 (后端保证唯一来源, 重复仅幂等刷新)。
      const unlistenDerived = await listen<unknown>('graph:derived', (event) => {
        const parsed = parseGraphDerivedEvent(event.payload);
        if (!parsed) return;
        get().setDerived(parsed.nodes);
      });

      // graph:compile — 后端 cmd_graph 编译队列状态广播;
      // 写 compileStatus 切片 (状态栏 / tab 角标 / 画布错误高亮的单一权威)。
      const unlistenCompile = await listen<unknown>('graph:compile', (event) => {
        const parsed = parseGraphCompileEvent(event.payload);
        if (!parsed) return;
        get().setCompileEvent(parsed);
        // refetch HIR (供 compile-results Tab 渲染);
        // 仅在编译成功时触发 — 错误态保留上次成功 HIR, 错误详情由 compile-errors Tab 单独展示
        if (parsed.state === 'ok') {
          void get().fetchHir(parsed.tab_id);
        }
      });

      // graph:source — 后端权威源图回推 (拓扑 op / MCP / 其他写入方提交成功后);
      // 画布按此收敛边与缺失的全局节点。提交在途时暂缓 (在途提交的响应/冲突路径已覆盖)
      const unlistenSource = await listen<unknown>('graph:source', (event) => {
        const parsed = parseGraphSourceEvent(event.payload);
        if (!parsed) return;
        if (isSyncInFlight(parsed.tab_id)) return;
        adoptSourceGraph(parsed);
      });

      // workspace:run — 工作区运行状态广播 (启动/暂停/停止, state + epoch);
      // 切片内的动作调用以命令返回值为准, 此处只做幂等刷新 (含 AI/MCP 写入方)
      const unlistenRun = await listen<{ state: RunState }>('workspace:run', (event) => {
        set({ runState: event.payload.state });
      });

      unlistenFns = [
        unlistenState,
        unlistenStats,
        unlistenChannels,
        unlistenDerived,
        unlistenCompile,
        unlistenSource,
        unlistenRun,
      ];

      const spectrumCoalescer = makeRafCoalescer<GraphStateSlice['spectrumResults']>(
        (v) => set({ spectrumResults: v })
      );
      const textOutputCoalescer = makeRafCoalescer<GraphStateSlice['customTextOutputs']>(
        (v) => set({ customTextOutputs: v })
      );

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
      storeUnsub = useAppStore.subscribe((s) => {
        if (s.rfNodes !== prevNodes) {
          prevNodes = s.rfNodes;
          reconcileSources();
        }
      });

      // tab 元数据 (控件 tab / 数据面板) 变化 → 整表覆盖到后端工作区。
      // 水合 / 历史恢复引发的批量覆盖同样入库; 基线取当前值, 避免启动即回推
      const metaKeyOf = (s: { controlTabs: unknown; dataTabs: unknown }) =>
        JSON.stringify([s.controlTabs, s.dataTabs]);
      let prevMetaKey = '';
      if (metaStoreUnsub) metaStoreUnsub();
      metaStoreUnsub = useAppStore.subscribe((s) => {
        const key = metaKeyOf(s);
        if (key === prevMetaKey) return;
        prevMetaKey = key;
        if (useAppStore.getState().workspaceReady) syncWorkspaceMeta();
      });

      // 工作区水合: 后端有持久化工作区时以权威快照覆盖本地 (图已在启动恢复时
      // 重编译, 不再初始同步); 否则按默认流程把空图推给后端
      let restored: boolean;
      try {
        restored = await hydrateWorkspaceFromBackend();
      } catch {
        restored = false;
      }
      set({ workspaceReady: true, workspaceRestored: restored });
      // 运行态水合 — 新建/重开项目后端默认 stopped, 这里取权威快照镜像
      void get().hydrateRunState();
      if (!restored) {
        get().controlTabs.forEach((tab) => { void get().syncTabGraph(tab.id); });
      }

      return () => {
        unlistenFns.forEach((fn) => fn());
        unlistenFns = [];
        spectrumCoalescer.cancel();
        cleanupSourceManagers();
        if (storeUnsub) {
          storeUnsub();
          storeUnsub = null;
        }
        if (metaStoreUnsub) {
          metaStoreUnsub();
          metaStoreUnsub = null;
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
      };
    },
  };
}
