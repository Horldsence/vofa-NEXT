import { beforeEach, describe, expect, it, vi } from 'vitest';
import { tauriMock } from '../../../test/setup';
import type { DecodedSampleBatch } from '../sampleProtocol';

interface WorkerPost {
  key: string;
  generation: number;
  receivedAt: number;
  buffer: ArrayBuffer;
}

class MockWorker {
  static instance: MockWorker;
  onmessage: ((event: MessageEvent) => void) | null = null;
  posts: WorkerPost[] = [];

  constructor() {
    MockWorker.instance = this;
  }

  postMessage(message: WorkerPost) {
    this.posts.push(message);
  }

  respond(post: WorkerPost, value: number) {
    const batch: DecodedSampleBatch = {
      sequence: value,
      status: 'live',
      rows: [{ seq: value, ts: value, value }],
      previewSkipped: 0,
      retentionEvicted: 0,
      ingressDropped: 0,
      byteLength: post.buffer.byteLength,
    };
    this.onmessage?.({
      data: {
        key: post.key,
        generation: post.generation,
        receivedAt: post.receivedAt,
        batch,
      },
    } as MessageEvent);
  }
}

function subscribeCalls() {
  return (
    tauriMock.invoke.mock.calls as unknown as [string, Record<string, unknown>][]
  ).filter(([command]) => command === 'subscribe_data');
}

beforeEach(() => {
  vi.resetModules();
  vi.stubGlobal('Worker', MockWorker);
  tauriMock.invoke.mockReset();
  tauriMock.invoke.mockResolvedValue(undefined);
});

describe('port sample client backpressure and lifecycle', () => {
  it('restarts after an in-flight unsubscribe and ignores the stale worker result', async () => {
    const { getPortSampleStore } = await import('../dataClient');
    const store = getPortSampleStore('protocol-restart', 'ch1');
    const firstUnsubscribe = store.subscribe(() => { return undefined; });
    const firstChannel = subscribeCalls()[0][1].onEvent as {
      onmessage: (buffer: ArrayBuffer) => void;
    };
    firstChannel.onmessage(new ArrayBuffer(1));
    const stalePost = MockWorker.instance.posts[0];

    firstUnsubscribe();
    const secondUnsubscribe = store.subscribe(() => { return undefined; });
    expect(subscribeCalls()).toHaveLength(2);
    const secondChannel = subscribeCalls()[1][1].onEvent as {
      onmessage: (buffer: ArrayBuffer) => void;
    };
    secondChannel.onmessage(new ArrayBuffer(2));

    MockWorker.instance.respond(stalePost, 1);
    expect(store.getSnapshot().rows).toEqual([]);
    const currentPost = MockWorker.instance.posts[1];
    MockWorker.instance.respond(currentPost, 2);
    const rows = store.getSnapshot().rows;
    expect(rows[rows.length - 1]?.value).toBe(2);

    secondUnsubscribe();
  });

  it('keeps only one in-flight and one replaceable pending decode per topic', async () => {
    const { getPortSampleStore } = await import('../dataClient');
    const store = getPortSampleStore('protocol-burst', 'ch2');
    const unsubscribe = store.subscribe(() => { return undefined; });
    const channel = subscribeCalls()[0][1].onEvent as {
      onmessage: (buffer: ArrayBuffer) => void;
    };

    for (let i = 1; i <= 10; i++) channel.onmessage(new ArrayBuffer(i));
    expect(MockWorker.instance.posts).toHaveLength(1);

    MockWorker.instance.respond(MockWorker.instance.posts[0], 1);
    expect(MockWorker.instance.posts).toHaveLength(2);
    MockWorker.instance.respond(MockWorker.instance.posts[1], 10);

    expect(store.getSnapshot().rows.map((row) => row.value)).toEqual([1, 10]);
    expect(store.getSnapshot().previewSkipped).toBe(8);
    unsubscribe();
  });

  it('restarts the channel and rejects a pre-reconnect worker result when resetting a source', async () => {
    const { getPortSampleStore, resetPortSampleStoresForSource } = await import('../dataClient');
    const store = getPortSampleStore('protocol-reset', 'ch3');
    const unsubscribe = store.subscribe(() => { return undefined; });
    const firstChannel = subscribeCalls()[0][1].onEvent as {
      onmessage: (buffer: ArrayBuffer) => void;
    };
    firstChannel.onmessage(new ArrayBuffer(1));
    const stalePost = MockWorker.instance.posts[0];

    resetPortSampleStoresForSource('protocol-reset');
    expect(subscribeCalls()).toHaveLength(2);
    expect(store.getSnapshot().rows).toEqual([]);

    const secondChannel = subscribeCalls()[1][1].onEvent as {
      onmessage: (buffer: ArrayBuffer) => void;
    };
    secondChannel.onmessage(new ArrayBuffer(2));
    MockWorker.instance.respond(stalePost, 1);
    expect(store.getSnapshot().rows).toEqual([]);

    const currentPost = MockWorker.instance.posts[1];
    MockWorker.instance.respond(currentPost, 2);
    expect(store.getSnapshot().rows[0]?.value).toBe(2);
    unsubscribe();
  });
});

describe('port sample store facade identity', () => {
  it('returns the same facade object for the same port key', async () => {
    const { getPortSampleStore } = await import('../dataClient');
    // useSyncExternalStore 依赖 subscribe/getSnapshot 引用稳定:
    // facade 每次新建会导致渲染→重订阅→快照替换的死循环。
    expect(getPortSampleStore('facade', 'ch1')).toBe(getPortSampleStore('facade', 'ch1'));
    expect(getPortSampleStore('facade', 'ch1')).not.toBe(getPortSampleStore('facade', 'ch2'));
    expect(getPortSampleStore('facade-a', 'ch1')).not.toBe(getPortSampleStore('facade-b', 'ch1'));
  });

  it('returns one shared facade for invalid ports', async () => {
    const { getPortSampleStore } = await import('../dataClient');
    expect(getPortSampleStore(undefined, 'ch1')).toBe(getPortSampleStore('', undefined));
  });
});
