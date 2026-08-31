import { useMemo, useSyncExternalStore } from 'react';
import type { Edge, Node } from '@xyflow/react';
import { useAppStore } from '../../store/appStore';
import {
  getPortSampleStore,
  type PortSampleSnapshot,
  type PortSampleStore,
} from '../data/dataClient';
import {
  numericPortRef,
  type NumericPortRef,
  type NumericPortState,
} from '../data/numericTypes';

const EMPTY_SNAPSHOT: PortSampleSnapshot = Object.freeze({
  version: 0,
  status: 'waiting',
  rows: [],
  previewSkipped: 0,
  retentionEvicted: 0,
  ingressDropped: 0,
  error: null,
});

const EMPTY_STORE: PortSampleStore = {
  subscribe: () => () => { return undefined; },
  getSnapshot: () => EMPTY_SNAPSHOT,
  clear: () => { return undefined; },
};

interface ResolvedNumericPort {
  ref: NumericPortRef | null;
  connected: boolean;
  legacyBinding: boolean;
}

/** 纯函数端口解析器；示波器和普通显示节点共享同一 edge 语义。 */
export function resolveNumericInputRef(
  edges: readonly Edge[],
  nodes: readonly Node[],
  widgetId: string,
  inputHandle: string,
  legacyChannel: number | null = null,
): ResolvedNumericPort {
  const edge = edges.find(
    (candidate) =>
      candidate.target === widgetId && candidate.targetHandle === inputHandle,
  );
  if (edge) {
    return {
      ref: numericPortRef(edge.source, edge.sourceHandle ?? 'value'),
      connected: true,
      legacyBinding: false,
    };
  }
  if (legacyChannel !== null) {
    const primary = nodes.find(
      (node) => node.type === 'protocol' && node.data?.global === true,
    );
    if (primary) {
      return {
        ref: numericPortRef(primary.id, `ch${legacyChannel}`),
        connected: false,
        legacyBinding: true,
      };
    }
  }
  return { ref: null, connected: false, legacyBinding: false };
}

function stateFromSnapshot(
  resolved: ResolvedNumericPort,
  snapshot: PortSampleSnapshot,
): NumericPortState {
  return {
    source: resolved.ref,
    connected: resolved.connected,
    legacyBinding: resolved.legacyBinding,
    status: snapshot.status,
    latest: snapshot.rows[snapshot.rows.length - 1] ?? null,
    history: snapshot.rows,
    previewSkipped: snapshot.previewSkipped,
    retentionEvicted: snapshot.retentionEvicted,
    ingressDropped: snapshot.ingressDropped,
    error: snapshot.error,
  };
}

function storeFor(ref: NumericPortRef | null): PortSampleStore {
  return ref ? getPortSampleStore(ref.nodeId, ref.handle) : EMPTY_STORE;
}

function useResolvedPort(resolved: ResolvedNumericPort): NumericPortState {
  const store = useMemo(
    () => storeFor(resolved.ref),
    [resolved.ref?.nodeId, resolved.ref?.handle],
  );
  const snapshot = useSyncExternalStore(
    store.subscribe,
    store.getSnapshot,
    store.getSnapshot,
  );
  return useMemo(
    () => stateFromSnapshot(resolved, snapshot),
    [resolved, snapshot],
  );
}

export function useNumericPort(ref: NumericPortRef | null): NumericPortState {
  const resolved = useMemo<ResolvedNumericPort>(
    () => ({ ref, connected: ref !== null, legacyBinding: false }),
    [ref?.nodeId, ref?.handle],
  );
  return useResolvedPort(resolved);
}

export function useNumericInput(
  widgetId: string,
  inputHandle = 'value',
  legacyChannel: number | null = null,
): NumericPortState {
  const edges = useAppStore((state) => state.rfEdges);
  const nodes = useAppStore((state) => state.rfNodes);
  const resolved = useMemo(
    () => resolveNumericInputRef(edges, nodes, widgetId, inputHandle, legacyChannel),
    [edges, nodes, widgetId, inputHandle, legacyChannel],
  );
  return useResolvedPort(resolved);
}

export function useNumericOutput(nodeId: string, outputHandle: string): NumericPortState {
  const resolved = useMemo<ResolvedNumericPort>(
    () => ({
      ref: numericPortRef(nodeId, outputHandle),
      connected: true,
      legacyBinding: false,
    }),
    [nodeId, outputHandle],
  );
  return useResolvedPort(resolved);
}

interface AggregateSnapshot {
  readonly snapshots: readonly PortSampleSnapshot[];
}

function aggregateStore(refs: readonly (NumericPortRef | null)[]) {
  const stores = refs.map(storeFor);
  let current: AggregateSnapshot = {
    snapshots: stores.map((store) => store.getSnapshot()),
  };
  return {
    subscribe(listener: () => void) {
      const unsubs = stores.map((store, index) =>
        store.subscribe(() => {
          const next = store.getSnapshot();
          if (current.snapshots[index] === next) return;
          const snapshots = [...current.snapshots];
          snapshots[index] = next;
          current = { snapshots };
          listener();
        }),
      );
      return () => unsubs.forEach((unsubscribe) => unsubscribe());
    },
    getSnapshot: () => current,
  };
}

/** 动态多端口 Hook：内部组合 external stores，不在循环中调用 React Hook。 */
export function useNumericInputs<const T extends readonly string[]>(
  widgetId: string,
  inputHandles: T,
  legacyChannels?: readonly (number | null)[],
): Readonly<Record<T[number], NumericPortState>> {
  const edges = useAppStore((state) => state.rfEdges);
  const nodes = useAppStore((state) => state.rfNodes);
  const handlesKey = inputHandles.join('\u0000');
  const legacyKey = legacyChannels?.join('\u0000') ?? '';
  const resolved = useMemo(
    () => inputHandles.map((handle, index) =>
      resolveNumericInputRef(edges, nodes, widgetId, handle, legacyChannels?.[index] ?? null)
    ),
    [edges, nodes, widgetId, handlesKey, legacyKey],
  );
  const refsKey = resolved
    .map(({ ref }) => (ref ? `${ref.nodeId}\u0001${ref.handle}` : ''))
    .join('\u0000');
  const aggregate = useMemo(
    () => aggregateStore(resolved.map(({ ref }) => ref)),
    [refsKey],
  );
  const snapshot = useSyncExternalStore(
    aggregate.subscribe,
    aggregate.getSnapshot,
    aggregate.getSnapshot,
  );
  return useMemo(() => {
    const result: Record<string, NumericPortState> = {};
    for (let index = 0; index < inputHandles.length; index++) {
      result[inputHandles[index]] = stateFromSnapshot(
        resolved[index],
        snapshot.snapshots[index] ?? EMPTY_SNAPSHOT,
      );
    }
    return result as Readonly<Record<T[number], NumericPortState>>;
  }, [handlesKey, resolved, snapshot]);
}

export const useNumericHistory = useNumericInputs;
