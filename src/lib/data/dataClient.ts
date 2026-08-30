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
  rows: Array<{ seq: number; ts: number; value: number }>;
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
  starting: boolean;
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
      batch?: DecodedSampleBatch;
      error?: string;
    }>,
  ) => {
    const entry = entries.get(event.data.key);
    if (!entry) return;
    if (event.data.error) {
      updateEntry(entry, undefined, event.data.error);
    } else if (event.data.batch) {
      updateEntry(entry, event.data.batch);
    }
  };
  return decoderWorker;
}

function updateEntry(
  entry: Entry,
  batch?: DecodedSampleBatch,
  error: string | null = null,
) {
  const start = performance.now();
  if (batch) {
    const rows =
      batch.status === 'live'
        ? [...entry.snapshot.rows, ...batch.rows]
        : [...batch.rows];
    entry.snapshot = {
      version: entry.snapshot.version + 1,
      status: batch.status,
      rows:
        rows.length > MAX_PREVIEW_ROWS ? rows.slice(-MAX_PREVIEW_ROWS) : rows,
      previewSkipped: batch.previewSkipped,
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
      renderMs: performance.now() - start,
    });
  }
}

function normalizeBuffer(value: ArrayBuffer | Uint8Array): ArrayBuffer {
  if (value instanceof ArrayBuffer) return value;
  return value.buffer.slice(
    value.byteOffset,
    value.byteOffset + value.byteLength,
  ) as ArrayBuffer;
}

function start(entry: Entry) {
  if (entry.channel || entry.starting) return;
  entry.starting = true;
  const channel = new Channel<ArrayBuffer>();
  entry.channel = channel;
  channel.onmessage = (message) => {
    const buffer = normalizeBuffer(message as ArrayBuffer | Uint8Array);
    const worker = getWorker();
    if (worker) {
      worker.postMessage({ key: entry.key, buffer }, [buffer]);
      return;
    }
    try {
      updateEntry(entry, decodeSampleEnvelope(buffer));
    } catch (error) {
      updateEntry(
        entry,
        undefined,
        error instanceof Error ? error.message : String(error),
      );
    }
  };
  void invoke('subscribe_data', {
    request: {
      kind: 'port_samples',
      source_node_id: entry.sourceNodeId,
      source_handle: entry.sourceHandle,
    },
    onEvent: channel,
    intervalMs: null,
    maxItems: null,
  })
    .catch((error: unknown) => updateEntry(entry, undefined, String(error)))
    .finally(() => {
      entry.starting = false;
    });
}

function stop(entry: Entry) {
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
      starting: false,
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
