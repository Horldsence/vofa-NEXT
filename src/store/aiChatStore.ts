import { create } from 'zustand';
import { Channel } from '@tauri-apps/api/core';
import { api } from '../lib/tauri/tauri';
import { notify } from '../lib/tauri/notifications';
import { formatAiKindError } from '../lib/ai/aiErrors';
import { checkAiProviderSettings } from '../settings/aiProvider';
import { useSettingsStore } from './settingsStore';
import { useAppStore } from './appStore';
import { t } from '../i18n';
import type {
  AiChatEvent,
  AiSessionMeta,
  AiToolRun,
  AiViewItem,
  McpServerConfig,
  McpToolInfo,
} from '../types';

interface AiChatState {
  /** 会话摘要列表 (后端持有) */
  sessions: AiSessionMeta[];
  /** 当前会话 id */
  activeSessionId: string | null;
  /** 当前会话的视图条目 — 由后端会话水合 */
  viewItems: AiViewItem[];
  /** 正在流式生成 (全局同一时刻至多一个任务) */
  streaming: boolean;
  /** 流式回合所属会话 — 切换会话时气泡只在所属会话内显示 */
  streamingSessionId: string | null;
  /** 当前流式文本聚合 */
  streamingText: string;
  /** 当前推理文本聚合 */
  reasoningText: string;
  /** 当前回合的工具调用记录 */
  toolRuns: AiToolRun[];
  /** 进行中的 task_id (可取消) */
  taskId: string | null;
  /** 聚合工具列表缓存 */
  tools: McpToolInfo[];
  /** 外部 server 配置缓存 */
  servers: McpServerConfig[];
  /** 本地 MCP server 状态 */
  serverRunning: boolean;
  serverPort: number | null;

  refreshSessions: () => Promise<void>;
  createSession: (title: string) => Promise<void>;
  switchSession: (id: string) => Promise<void>;
  renameSession: (id: string, title: string) => Promise<void>;
  deleteSession: (id: string) => Promise<void>;
  send: (text: string) => Promise<void>;
  /** 重新生成 — 截掉最后一条用户条目之后的回合并重跑 */
  regenerate: () => Promise<void>;
  cancel: () => Promise<void>;
  clearSession: () => Promise<void>;
  refreshTools: () => Promise<void>;
  refreshServers: () => Promise<void>;
  refreshServerStatus: () => Promise<void>;
  startLocalServer: () => Promise<void>;
  stopLocalServer: () => Promise<void>;
  addServer: (config: McpServerConfig) => Promise<void>;
  removeServer: (id: string) => Promise<void>;
  setServerEnabled: (id: string, enabled: boolean) => Promise<void>;
}

/** 从设置读取 provider 配置 (随请求传给后端, 后端不落盘) */
function providerConfigFromSettings() {
  const ai = useSettingsStore.getState().settings.ai;
  return {
    adapter: ai.adapter,
    base_url: ai.baseUrl,
    api_key: ai.apiKey,
    model: ai.model,
    temperature: ai.temperature,
    max_tokens: ai.maxTokens,
  };
}

/** 命令级失败 (invoke reject 的结构化错误对象) → 本地错误条目 (kind/data 透传, 渲染时本地化) */
function errorItemFromRejection(err: unknown): AiViewItem {
  if (err && typeof err === 'object' && 'kind' in err) {
    const e = err as { message?: string; kind?: string; data?: Record<string, string> };
    return {
      role: 'assistant',
      text: e.message ?? String(err),
      error: true,
      error_kind: e.kind,
      error_data: e.data,
    };
  }
  return { role: 'assistant', text: err instanceof Error ? err.message : String(err), error: true };
}

export const useAiChatStore = create<AiChatState>()((set, get) => {
  /** 后端条目 → 视图条目 (同构, 仅收紧类型) */
  const toViewItems = (items: AiViewItem[]): AiViewItem[] =>
    items.map((it) => ({ ...it, tools: it.tools?.map((t) => ({ ...t })) }));

  /**
   * 发起一次对话任务 (send / regenerate 共用)。
   * `failLocal`: 命令级失败 (配置错误等, 无事件流) 时的本地呈现。
   */
  const startTask = async (
    sessionId: string,
    payload: { text: string | null; regenerate: boolean },
    failLocal: (err: unknown) => void
  ) => {
    const ai = useSettingsStore.getState().settings.ai;
    const channel = new Channel<AiChatEvent>();
    channel.onmessage = (event) => {
      switch (event.type) {
        case 'delta':
          set((s) => ({ streamingText: s.streamingText + event.text }));
          break;
        case 'reasoning_delta':
          set((s) => ({ reasoningText: s.reasoningText + event.text }));
          break;
        case 'tool_call':
          set((s) => ({
            toolRuns: [
              ...s.toolRuns,
              { id: event.id, name: event.name, arguments: event.arguments, content: '', is_error: false, done: false },
            ],
          }));
          break;
        case 'tool_result':
          set((s) => ({
            toolRuns: s.toolRuns.map((r) =>
              r.id === event.id ? { ...r, content: event.content, is_error: event.is_error, done: true } : r
            ),
          }));
          break;
        case 'done':
        case 'cancelled':
        case 'error':
          void finishTurn();
          break;
      }
    };

    try {
      const lang = useAppStore.getState().lang;
      const taskId = await api.aiChatSend(
        sessionId,
        payload.text,
        payload.regenerate,
        providerConfigFromSettings(),
        ai.systemPrompt.trim() || null,
        ai.maxToolRounds,
        ai.mcpToolsEnabled,
        ai.builtinToolsEnabled,
        lang,
        channel
      );
      set({ taskId });
    } catch (e) {
      failLocal(e);
    }
  };

  /** 回合收束 (done/cancelled/error 共用) — 后端已落盘, 拉取权威视图对账 */
  const finishTurn = async () => {
    const s = get();
    const patch = {
      streaming: false,
      streamingText: '',
      reasoningText: '',
      toolRuns: [],
      taskId: null,
      streamingSessionId: null,
    };
    // 后端不可达 (HMR 场景): 退化为本地聚合, 不丢已收到的内容
    const foldLocal = (items: AiViewItem[]): AiViewItem[] => {
      const tools = s.toolRuns.map((r) => ({ ...r }));
      const out = [...items];
      if (s.streamingText || tools.length > 0) {
        out.push({ role: 'assistant', text: s.streamingText, tools });
      }
      return out;
    };
    if (!s.activeSessionId || s.activeSessionId !== s.streamingSessionId) {
      set(patch);
      return;
    }
    try {
      const session = await api.chatGetSession(s.activeSessionId);
      set({ ...patch, viewItems: toViewItems(session?.items ?? []) });
    } catch {
      set((cur) => ({ ...patch, viewItems: foldLocal(cur.viewItems) }));
    }
  };

  return {
    sessions: [],
    activeSessionId: null,
    viewItems: [],
    streaming: false,
    streamingSessionId: null,
    streamingText: '',
    reasoningText: '',
    toolRuns: [],
    taskId: null,
    tools: [],
    servers: [],
    serverRunning: false,
    serverPort: null,

    refreshSessions: async () => {
      try {
        const sessions = await api.chatListSessions();
        const { activeSessionId } = get();
        set({
          sessions,
          // 当前会话已被删除 (他端) 时回退到最近一个会话
          activeSessionId: sessions.some((s) => s.id === activeSessionId)
            ? activeSessionId
            : (sessions[0]?.id ?? null),
        });
        if (get().activeSessionId) await get().switchSession(get().activeSessionId!);
      } catch {
        /* 后端不可达时静默 (HMR 场景) */
      }
    },

    createSession: async (title) => {
      try {
        const session = await api.chatCreateSession(title);
        set((s) => ({
          sessions: [
            ...s.sessions,
            {
              id: session.id,
              title: session.title,
              created_at: session.created_at,
              updated_at: session.updated_at,
              item_count: session.items.length,
            },
          ],
          activeSessionId: session.id,
          viewItems: [],
          streamingText: '',
          reasoningText: '',
          toolRuns: [],
        }));
      } catch {
        /* ignore */
      }
    },

    switchSession: async (id) => {
      set({ activeSessionId: id });
      try {
        const session = await api.chatGetSession(id);
        set({ viewItems: toViewItems(session?.items ?? []) });
      } catch {
        set({ viewItems: [] });
      }
    },

    renameSession: async (id, title) => {
      await api.chatRenameSession(id, title);
      set((s) => ({
        sessions: s.sessions.map((m) => (m.id === id ? { ...m, title } : m)),
      }));
    },

    deleteSession: async (id) => {
      await api.chatDeleteSession(id);
      const rest = get().sessions.filter((s) => s.id !== id);
      set({ sessions: rest });
      if (get().activeSessionId === id) {
        if (rest.length > 0) {
          await get().switchSession(rest[0].id);
        } else {
          set({ activeSessionId: null, viewItems: [] });
        }
      }
    },

    send: async (text) => {
      const { streaming, activeSessionId, viewItems } = get();
      const body = text.trim();
      if (streaming || !body || !activeSessionId) return;

      // 发送前检查 (与后端 validate_config 同规则): 配置缺失直接拦截, 不发请求
      const issue = checkAiProviderSettings(useSettingsStore.getState().settings.ai);
      if (issue) {
        const lang = useAppStore.getState().lang;
        const view = formatAiKindError(issue.kind, issue.params, '', lang);
        notify.error(t(lang, 'aiSendBlocked'), view.summary, { source: 'ai-send' });
        return;
      }

      set({
        viewItems: [...viewItems, { role: 'user', text: body }],
        streaming: true,
        streamingSessionId: activeSessionId,
        streamingText: '',
        reasoningText: '',
        toolRuns: [],
      });

      await startTask(
        activeSessionId,
        { text: body, regenerate: false },
        // 命令级失败: 后端未写入, 本地呈现结构化错误条目 (kind/data 透传供本地化)
        (err) =>
          set((s) => ({
            streaming: false,
            streamingSessionId: null,
            streamingText: '',
            reasoningText: '',
            toolRuns: [],
            taskId: null,
            viewItems: [...s.viewItems, errorItemFromRejection(err)],
          }))
      );
    },

    regenerate: async () => {
      const { streaming, activeSessionId, viewItems } = get();
      if (streaming || !activeSessionId) return;
      // 重发同样走发送前检查 (配置可能已失效)
      const issue = checkAiProviderSettings(useSettingsStore.getState().settings.ai);
      if (issue) {
        const lang = useAppStore.getState().lang;
        const view = formatAiKindError(issue.kind, issue.params, '', lang);
        notify.error(t(lang, 'aiSendBlocked'), view.summary, { source: 'ai-send' });
        return;
      }
      // 本地乐观截断: 移除最后一条用户条目之后的条目 (与后端 truncate 对称)
      let lastUser = -1;
      for (let i = viewItems.length - 1; i >= 0; i--) {
        if (viewItems[i].role === 'user') {
          lastUser = i;
          break;
        }
      }
      set({
        viewItems: lastUser >= 0 ? viewItems.slice(0, lastUser + 1) : viewItems,
        streaming: true,
        streamingSessionId: activeSessionId,
        streamingText: '',
        reasoningText: '',
        toolRuns: [],
      });

      await startTask(
        activeSessionId,
        { text: null, regenerate: true },
        (err) =>
          set((s) => ({
            streaming: false,
            streamingSessionId: null,
            streamingText: '',
            reasoningText: '',
            toolRuns: [],
            taskId: null,
            viewItems: [...s.viewItems, errorItemFromRejection(err)],
          }))
      );
    },

    cancel: async () => {
      const { taskId } = get();
      if (taskId) await api.aiChatCancel(taskId).catch(() => false);
    },

    clearSession: async () => {
      const { activeSessionId } = get();
      if (!activeSessionId) return;
      await api.chatClearSession(activeSessionId);
      set({
        viewItems: [],
        streamingText: '',
        reasoningText: '',
        toolRuns: [],
        taskId: null,
        streaming: false,
      });
    },

    refreshTools: async () => {
      try {
        const [tools, servers] = await Promise.all([api.mcpListTools(), api.mcpListServers()]);
        set({ tools, servers });
      } catch {
        /* 后端不可达时静默 (HMR 场景) */
      }
    },

    refreshServers: async () => {
      try {
        set({ servers: await api.mcpListServers() });
      } catch {
        /* ignore */
      }
    },

    refreshServerStatus: async () => {
      try {
        const st = await api.mcpServerStatus();
        set({ serverRunning: st.running, serverPort: st.port });
      } catch {
        /* ignore */
      }
    },

    startLocalServer: async () => {
      const port = useSettingsStore.getState().settings.ai.mcpServerPort;
      try {
        const bound = await api.mcpServerStart(port);
        set({ serverRunning: true, serverPort: bound });
      } catch {
        set({ serverRunning: false });
      }
    },

    stopLocalServer: async () => {
      try {
        await api.mcpServerStop();
      } finally {
        set({ serverRunning: false, serverPort: null });
      }
    },

    addServer: async (config) => {
      await api.mcpAddServer(config);
      await get().refreshServers();
    },

    removeServer: async (id) => {
      api.mcpRemoveServer(id);
      await get().refreshServers();
      await get().refreshTools();
    },

    setServerEnabled: async (id, enabled) => {
      await api.mcpSetServerEnabled(id, enabled);
      await get().refreshServers();
      await get().refreshTools();
    },
  };
});
