import { describe, expect, it, beforeEach } from 'vitest';
import { tauriMock } from '../../test/setup';
import { subscribeDisplay } from '../buffers/shardedSubscription';
import type { Mock } from 'vitest';

interface Batch { seq: number; tag: string }

beforeEach(() => {
  tauriMock.invoke.mockReset();
});

describe('subscribeDisplay (统一单通道显示订阅)', () => {
  // tauriMock.invoke 声明为 () => Promise<undefined>, 此处放宽为任意 mock 以便自定义返回值/读参数
  const invokeMock = tauriMock.invoke as unknown as Mock;
  const subCalls = () =>
    (invokeMock.mock.calls as [string, Record<string, unknown>][]).filter(
      (c) => c[0] === 'subscribe_data',
    );

  it('每个逻辑订阅只创建一个 Channel', async () => {
    invokeMock.mockResolvedValue({
      subscription_id: 1,
      schema_version: 1,
      mode: 'json',
    });
    const sub = subscribeDisplay<Batch>(
      { kind: 'can_frames' },
      'can_frames',
      () => { return undefined; },
    );
    await new Promise((r) => setTimeout(r, 0));

    const calls = subCalls();
    expect(calls).toHaveLength(1);
    expect(calls[0][1].request).toEqual({ kind: 'can_frames' });
    expect(calls[0][1]).toHaveProperty('onEvent');

    sub.cancel();
    await new Promise((r) => setTimeout(r, 0));
    const unsubCalls = (invokeMock.mock.calls as [string][]).filter(
      (c) => c[0] === 'unsubscribe_data',
    );
    expect(unsubCalls).toHaveLength(1);
  });
});
