import { describe, expect, it, beforeEach } from 'vitest';
import { tauriMock } from '../../test/setup';
import {
  makeLatestSink,
  makeOrderedSink,
  subscribeSharded,
  STREAM_SHARDS,
} from '../buffers/shardedSubscription';

type Batch = { seq: number; tag: string };
const b = (seq: number): Batch => ({ seq, tag: `b${seq}` });

beforeEach(() => {
  tauriMock.invoke.mockReset();
});

describe('makeOrderedSink (增量流严格重组)', () => {
  it('顺序到达直接交付', () => {
    const got: number[] = [];
    const sink = makeOrderedSink<Batch>((x) => got.push(x.seq));
    for (let i = 0; i < 5; i++) sink(b(i));
    expect(got).toEqual([0, 1, 2, 3, 4]);
  });

  it('乱序到达先缓冲, 缺口补齐后按序交付', () => {
    const got: number[] = [];
    const sink = makeOrderedSink<Batch>((x) => got.push(x.seq));
    sink(b(2));
    sink(b(0));
    sink(b(3));
    expect(got).toEqual([0]); // 2,3 在等 seq 1
    sink(b(1));
    expect(got).toEqual([0, 1, 2, 3]);
  });

  it('过期/重复批次被丢弃', () => {
    const got: number[] = [];
    const sink = makeOrderedSink<Batch>((x) => got.push(x.seq));
    sink(b(0));
    sink(b(1));
    sink(b(0)); // 重复
    sink(b(1)); // 过期
    sink(b(2));
    expect(got).toEqual([0, 1, 2]);
  });

  it('seq 缺口积压超阈值后跳到最小可用序号 (防卡死)', () => {
    const got: number[] = [];
    const sink = makeOrderedSink<Batch>((x) => got.push(x.seq));
    sink(b(0));
    // seq 1 永久缺失, 灌入 65 条后续批次触发兜底
    for (let i = 2; i <= 66; i++) sink(b(i));
    expect(got[0]).toBe(0);
    expect(got).toContain(2);
    expect(got).toContain(66);
    expect(got).toHaveLength(66); // 0 + 2..66
  });
});

describe('makeLatestSink (快照流最新胜出)', () => {
  it('只交付 seq 递增的快照', () => {
    const got: number[] = [];
    const sink = makeLatestSink<Batch>((x) => got.push(x.seq));
    sink(b(0));
    sink(b(2));
    sink(b(1)); // 乱序旧快照丢弃
    sink(b(2)); // 重复丢弃
    sink(b(3));
    expect(got).toEqual([0, 2, 3]);
  });
});

describe('subscribeSharded (分片组建组/加入/取消)', () => {
  // tauriMock.invoke 声明为 () => Promise<undefined>, 此处放宽为任意 mock 以便自定义返回值/读参数
  const invokeMock = tauriMock.invoke as unknown as import('vitest').Mock;
  const subCalls = () =>
    (invokeMock.mock.calls as [string, Record<string, unknown>][]).filter(
      (c) => c[0] === 'subscribe_x'
    );

  it('首个 Channel 建组, 其余凭组 id 加入', async () => {
    invokeMock.mockResolvedValue('g1');
    const sub = subscribeSharded<Batch>('subscribe_x', 'unsubscribe_x', {}, () => {});
    await new Promise((r) => setTimeout(r, 0));

    const calls = subCalls();
    expect(calls).toHaveLength(STREAM_SHARDS);
    expect(calls[0][1]).not.toHaveProperty('groupId');
    expect(calls[0][1]).toHaveProperty('onEvent');
    for (let i = 1; i < STREAM_SHARDS; i++) {
      expect(calls[i][1]).toMatchObject({ groupId: 'g1' });
    }

    sub.cancel();
    await new Promise((r) => setTimeout(r, 0));
    const unsubCalls = (invokeMock.mock.calls as [string][]).filter(
      (c) => c[0] === 'unsubscribe_x'
    );
    expect(unsubCalls).toHaveLength(STREAM_SHARDS);
  });

  it('后端返回空组 id (no-op) 时不再加入额外分片', async () => {
    invokeMock.mockResolvedValue('');
    subscribeSharded<Batch>('subscribe_x', 'unsubscribe_x', {}, () => {});
    await new Promise((r) => setTimeout(r, 0));
    expect(subCalls()).toHaveLength(1);
  });
});
