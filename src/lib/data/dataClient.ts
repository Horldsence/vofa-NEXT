import { Channel, invoke } from '@tauri-apps/api/core';
import { closeTauriChannel } from '../tauri/tauri';
import {
  decodeSampleEnvelope,
  type DecodedSampleBatch,
  type PortSampleStatus,
} from './sampleProtocol';

const MAX_PREVIEW_ROWS = 500;

export interface PortSampleSnapshot {
  version: number;
  status: PortSampleStatus;
  rows: { seq: number; ts: number; value: number }[];
  previewSkipped: number;
  retentionEvicted: number;
  ingressDropped: number;
  error: string | null;
}

interface Entry {
  key: string;
  sourceNodeId: string;
  sourceHandle: string;
  channel: Channel<ArrayBuffer> | null;
  listeners: Set<() => void>;
  snapshot: PortSampleSnapshot;
  generation: number;
  startingGeneration: number | null;
  inFlightGeneration: number | null;
  pendingDecode: DecodeJob | null;
  localPreviewSkipped: number;
}

interface DecodeJob {
  buffer: ArrayBuffer;
  generation: number;
  receivedAt: number;
}

const EMPTY_SNAPSHOT: PortSampleSnapshot = Object.freeze({
  version: 0,
  status: 'waiting',
  rows: [],
  previewSkipped: 0,
  retentionEvicted: 0,
  ingressDropped: 0,
  error: null,
});

const entries = new Map<string, Entry>();
let decoderWorker: Worker | null | undefined;

function topicKey(sourceNodeId: string, sourceHandle: string): string {
  return `${sourceNodeId}\u0000${sourceHandle}`;
}

function getWorker(): Worker | null {
  if (decoderWorker !== undefined) return decoderWorker;
  if (typeof Worker === 'undefined') {
    decoderWorker = null;
    return null;
  }
  decoderWorker = new Worker(
    new URL('./sampleDecode.worker.ts', import.meta.url),
    {
      type: 'module',
    },
  );
  decoderWorker.onmessage = (
    event: MessageEvent<{
      key: string;
      generation: number;
      receivedAt: number;
      batch?: DecodedSampleBatch;
      error?: string;
    }>,
  ) => {
    const entry = entries.get(event.data.key);
    if (!entry) return;
    if (entry.inFlightGeneration !== event.data.generation) return;
    entry.inFlightGeneration = null;
    if (entry.generation === event.data.generation) {
      if (event.data.error) {
        updateEntry(entry, undefined, event.data.error);
      } else if (event.data.batch) {
        updateEntry(
          entry,
          event.data.batch,
          null,
          performance.now() - event.data.receivedAt,
        );
      }
    }
    dispatchPendingDecode(entry);
  };
  return decoderWorker;
}

function updateEntry(
  entry: Entry,
  batch?: DecodedSampleBatch,
  error: string | null = null,
  elapsedMs = 0,
) {
  const start = performance.now();
  if (batch) {
    // 状态事件与数值正交：disconnect/out-of-range 等空批次保留最后有效样本，
    // 避免 UI 把断流误显示为 0。显式 clear/reset 仍会清空历史。
    const rows = batch.rows.length > 0
      ? [...entry.snapshot.rows, ...batch.rows]
      : entry.snapshot.rows;
    entry.snapshot = {
      version: entry.snapshot.version + 1,
      status: batch.status,
      rows:
        rows.length > MAX_PREVIEW_ROWS ? rows.slice(-MAX_PREVIEW_ROWS) : rows,
      previewSkipped: batch.previewSkipped + entry.localPreviewSkipped,
      retentionEvicted: batch.retentionEvicted,
      ingressDropped: batch.ingressDropped,
      error,
    };
  } else {
    entry.snapshot = {
      ...entry.snapshot,
      version: entry.snapshot.version + 1,
      error,
    };
  }
  for (const listener of entry.listeners) listener();
  if (batch && entry.channel) {
    void invoke('ack_data', {
      subscriptionId: entry.channel.id,
      sequence: batch.sequence,
      bufferedBytes: batch.byteLength,
      renderMs: elapsedMs + performance.now() - start,
    });
  }
}

function normalizeBuffer(value: ArrayBuffer | Uint8Array): ArrayBuffer {
  if (value instanceof ArrayBuffer) return value;
  return value.buffer.slice(
    value.byteOffset,
    value.byteOffset + value.byteLength,
  );
}

function dispatchPendingDecode(entry: Entry) {
  if (entry.inFlightGeneration !== null) return;
  const job = entry.pendingDecode;
  entry.pendingDecode = null;
  if (!job || job.generation !== entry.generation) return;
  const worker = getWorker();
  if (!worker) {
    try {
      updateEntry(
        entry,
        decodeSampleEnvelope(job.buffer),
        null,
        performance.now() - job.receivedAt,
      );
    } catch (error) {
      updateEntry(
        entry,
        undefined,
        error instanceof Error ? error.message : String(error),
      );
    }
    return;
  }
  entry.inFlightGeneration = job.generation;
  worker.postMessage(
    {
      key: entry.key,
      generation: job.generation,
      receivedAt: job.receivedAt,
      buffer: job.buffer,
    },
    [job.buffer],
  );
}

function enqueueDecode(entry: Entry, job: DecodeJob) {
  if (job.generation !== entry.generation) return;
  if (entry.pendingDecode) entry.localPreviewSkipped++;
  entry.pendingDecode = job;
  dispatchPendingDecode(entry);
}

function start(entry: Entry) {
  if (entry.channel || entry.startingGeneration !== null) return;
  const generation = entry.generation + 1;
  entry.generation = generation;
  entry.startingGeneration = generation;
  entry.localPreviewSkipped = 0;
  entry.snapshot = {
    ...EMPTY_SNAPSHOT,
    version: entry.snapshot.version + 1,
  };
  const channel = new Channel<ArrayBuffer>();
  entry.channel = channel;
  channel.onmessage = (message) => {
    if (entry.generation !== generation || entry.channel !== channel) return;
    const buffer = normalizeBuffer(message);
    enqueueDecode(entry, { buffer, generation, receivedAt: performance.now() });
  };
  void invoke('subscribe_data', {
    request: {
      kind: 'port_samples',
      source_node_id: entry.sourceNodeId,
      source_handle: entry.sourceHandle,
    },
    onEvent: channel,
    intervalMs: 17,
    maxItems: MAX_PREVIEW_ROWS,
  })
    .catch((error: unknown) => {
      if (entry.generation === generation) {
        updateEntry(entry, undefined, String(error));
      }
    })
    .finally(() => {
      if (entry.startingGeneration === generation) {
        entry.startingGeneration = null;
      }
      if (
        entry.generation !== generation ||
        entry.channel !== channel ||
        entry.listeners.size === 0
      ) {
        void closeTauriChannel(channel, 'unsubscribe_data', channel.id);
      }
      if (entry.listeners.size > 0 && !entry.channel) start(entry);
    });
}

function stop(entry: Entry) {
  entry.generation++;
  entry.startingGeneration = null;
  entry.pendingDecode = null;
  const channel = entry.channel;
  entry.channel = null;
  if (channel) void closeTauriChannel(channel, 'unsubscribe_data', channel.id);
}

export interface PortSampleStore {
  subscribe: (listener: () => void) => () => void;
  getSnapshot: () => PortSampleSnapshot;
  clear: () => void;
}

export function getPortSampleStore(
  sourceNodeId: string | undefined,
  sourceHandle: string | undefined,
): PortSampleStore {
  if (!sourceNodeId || !sourceHandle) {
    return {
      subscribe: () => () => {},
      getSnapshot: () => EMPTY_SNAPSHOT,
      clear: () => {},
    };
  }
  const key = topicKey(sourceNodeId, sourceHandle);
  let entry = entries.get(key);
  if (!entry) {
    entry = {
      key,
      sourceNodeId,
      sourceHandle,
      channel: null,
      listeners: new Set(),
      snapshot: EMPTY_SNAPSHOT,
      generation: 0,
      startingGeneration: null,
      inFlightGeneration: null,
      pendingDecode: null,
      localPreviewSkipped: 0,
    };
    entries.set(key, entry);
  }
  const target = entry;
  return {
    subscribe(listener) {
      target.listeners.add(listener);
      if (target.listeners.size === 1) start(target);
      return () => {
        target.listeners.delete(listener);
        if (target.listeners.size === 0) stop(target);
      };
    },
    getSnapshot: () => target.snapshot,
    clear() {
      target.snapshot = {
        ...EMPTY_SNAPSHOT,
        version: target.snapshot.version + 1,
      };
      for (const listener of target.listeners) listener();
    },
  };
}

export function resetPortSampleStoresForSource(sourceNodeId: string): void {
  for (const entry of entries.values()) {
    if (entry.sourceNodeId !== sourceNodeId) continue;
    const shouldRestart = entry.listeners.size > 0;
    stop(entry);
    entry.snapshot = {
      ...EMPTY_SNAPSHOT,
      version: entry.snapshot.version + 1,
    };
    entry.localPreviewSkipped = 0;
    entry.pendingDecode = null;
    for (const listener of entry.listeners) listener();
    if (shouldRestart) start(entry);
  }
}
