import { beforeEach, describe, expect, it, vi } from 'vitest';
import { tauriMock } from '../../test/setup';
import { useAiChatStore } from '../aiChatStore';

/// 多会话 store — 历史所有权在后端, 前端只做乐观视图 + 终态对账

// invoke 桩被声明为 () => Promise<undefined>, 按值/带参实现需透传 cast
// (与 updateStore.test / syncTabGraph.test 同款写法)
function mockInvokeReturn(value: unknown): void {
  (tauriMock.invoke as unknown as { mockResolvedValue: (v: unknown) => void }).mockResolvedValue(value);
}
function mockInvokeReject(error: unknown): void {
  (tauriMock.invoke as unknown as { mockRejectedValue: (v: unknown) => void }).mockRejectedValue(error);
}
function mockInvokeImpl(impl: (cmd: string) => Promise<unknown>): void {
  tauriMock.invoke.mockImplementation(impl as unknown as () => Promise<undefined>);
}

function lastInvokeCall(cmd: string): Record<string, unknown> {
  const calls = tauriMock.invoke.mock.calls as unknown as [string, Record<string, unknown>][];
  const call = calls.find(([c]) => c === cmd);
  if (!call) throw new Error(`未找到 ${cmd} 调用`);
  return call[1];
}

const RESET = {
  sessions: [],
  activeSessionId: null,
  viewItems: [],
  streaming: false,
  streamingSessionId: null,
  streamingText: '',
  reasoningText: '',
  toolRuns: [],
  taskId: null,
};

describe('aiChatStore 多会话 (后端持有)', () => {
  beforeEach(() => {
    tauriMock.invoke.mockReset();
    mockInvokeReturn(undefined);
    useAiChatStore.setState(RESET);
  });

  it('refreshSessions 拉取摘要并水合当前会话条目', async () => {
    mockInvokeImpl(async (cmd: string) => {
      if (cmd === 'chat_list_sessions') {
        return [{ id: 's1', title: '调试', created_at: 1, updated_at: 2, item_count: 1 }];
      }
      if (cmd === 'chat_get_session') {
        return {
          id: 's1',
          title: '调试',
          created_at: 1,
          updated_at: 2,
          items: [{ role: 'user', text: '你好' }],
        };
      }
      return undefined;
    });

    await useAiChatStore.getState().refreshSessions();

    const s = useAiChatStore.getState();
    expect(s.activeSessionId).toBe('s1');
    expect(s.sessions).toHaveLength(1);
    expect(s.viewItems).toEqual([{ role: 'user', text: '你好' }]);
  });

  it('send 把 sessionId/text 传给后端并乐观追加用户条目', async () => {
    useAiChatStore.setState({ ...RESET, activeSessionId: 's1' });
    mockInvokeReturn('task-1');

    await useAiChatStore.getState().send('你好');

    const s = useAiChatStore.getState();
    expect(s.viewItems).toEqual([{ role: 'user', text: '你好' }]);
    expect(s.streaming).toBe(true);
    expect(s.streamingSessionId).toBe('s1');
    const args = lastInvokeCall('ai_chat_send');
    expect(args.sessionId).toBe('s1');
    expect(args.text).toBe('你好');
    expect(args.regenerate).toBe(false);
  });

  it('done 事件后从后端拉取权威视图 (含工具卡片)', async () => {
    useAiChatStore.setState({ ...RESET, activeSessionId: 's1' });
    mockInvokeImpl(async (cmd: string) => {
      if (cmd === 'ai_chat_send') return 'task-1';
      if (cmd === 'chat_get_session') {
        return {
          id: 's1',
          title: 'x',
          created_at: 1,
          updated_at: 2,
          items: [
            { role: 'user', text: '你好' },
            {
              role: 'assistant',
              text: '答案是 42',
              tools: [{ id: 'c1', name: 'probe', arguments: {}, content: '42', is_error: false, done: true }],
            },
          ],
        };
      }
      return undefined;
    });

    await useAiChatStore.getState().send('你好');
    const args = lastInvokeCall('ai_chat_send') as {
      onEvent: { onmessage: ((e: unknown) => void) | null };
    };
    args.onEvent.onmessage?.({ type: 'done', rounds: 1 });
    await vi.waitFor(() => expect(useAiChatStore.getState().streaming).toBe(false));

    const s = useAiChatStore.getState();
    expect(s.viewItems).toHaveLength(2);
    expect(s.viewItems[1].text).toBe('答案是 42');
    expect(s.viewItems[1].tools?.[0].done).toBe(true);
  });

  it('命令级失败 (配置错误): 流式态复位 + 本地错误条目', async () => {
    useAiChatStore.setState({ ...RESET, activeSessionId: 's1' });
    mockInvokeReject(new Error('缺少 API key'));

    await useAiChatStore.getState().send('你好');

    const s = useAiChatStore.getState();
    expect(s.streaming).toBe(false);
    expect(s.viewItems).toHaveLength(2);
    expect(s.viewItems[1]).toMatchObject({ role: 'assistant', error: true });
  });

  it('regenerate 乐观截断最后一条用户之后的条目, 后端走 regenerate 通道', async () => {
    useAiChatStore.setState({
      ...RESET,
      activeSessionId: 's1',
      viewItems: [
        { role: 'user', text: '问' },
        { role: 'assistant', text: '错答', error: true },
      ],
    });
    mockInvokeReturn('task-2');

    await useAiChatStore.getState().regenerate();

    expect(useAiChatStore.getState().viewItems).toEqual([{ role: 'user', text: '问' }]);
    const args = lastInvokeCall('ai_chat_send');
    expect(args.regenerate).toBe(true);
    expect(args.text).toBeNull();
  });

  it('deleteSession 删除当前会话后自动切换到剩余会话', async () => {
    useAiChatStore.setState({
      ...RESET,
      activeSessionId: 's1',
      sessions: [
        { id: 's1', title: 'a', created_at: 1, updated_at: 1, item_count: 0 },
        { id: 's2', title: 'b', created_at: 2, updated_at: 2, item_count: 0 },
      ],
    });

    await useAiChatStore.getState().deleteSession('s1');

    expect(useAiChatStore.getState().activeSessionId).toBe('s2');
    expect(tauriMock.invoke).toHaveBeenCalledWith('chat_delete_session', { sessionId: 's1' });
  });
});
