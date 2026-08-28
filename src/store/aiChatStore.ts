import { create } from 'zustand';
import { Channel } from '@tauri-apps/api/core';
import { api } from '../lib/tauri/tauri';
import { useSettingsStore } from './settingsStore';
import type { AiChatEvent, AiChatMessage, AiToolCall, AiToolRun, McpServerConfig, McpToolInfo } from '../types';

/// 对话视图条目 — user / assistant (含本回合工具调用卡片)
export interface AiViewItem {
  role: 'user' | 'assistant';
  text: string;
  tools?: AiToolRun[];
  error?: boolean;
}

interface AiChatState {
  /** 视图条目 (含工具卡片), 关闭面板不丢失 */
  viewItems: AiViewItem[];
  /** 正在流式生成 */
  streaming: boolean;
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
  /** 面板可见性 */
  panelVisible: boolean;

  setPanelVisible: (v: boolean) => void;
  send: (text: string) => Promise<void>;
  cancel: () => Promise<void>;
  clear: () => void;
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

/** 原始对话历史 (发送给后端的完整上下文; 模块级, 不进视图) */
let history: AiChatMessage[] = [];

/** 把一个回合 (文本 + 工具调用) 还原为完整消息历史, 供下轮请求携带上下文 */
function exchangeToHistory(text: string, tools: AiToolRun[]): AiChatMessage[] {
  const calls: AiToolCall[] = tools.map((t) => ({ id: t.id, name: t.name, arguments: t.arguments }));
  const out: AiChatMessage[] = [
    { role: 'assistant', content: text, tool_calls: calls.length > 0 ? calls : undefined },
  ];
  for (const t of tools) {
    if (!t.done) break;
    out.push({ role: 'tool', content: t.content, tool_call_id: t.id, name: t.name });
  }
  return out;
}

export const useAiChatStore = create<AiChatState>()((set, get) => {
  /** 回合收束 (done/cancelled/error 共用): 沉淀视图条目 + 历史并复位流式状态 */
  const finishTurn = (extraError?: string) => {
    set((s) => {
      const tools = s.toolRuns.map((r) => ({ ...r }));
      const items = [...s.viewItems];
      if (s.streamingText || tools.length > 0) {
        items.push({ role: 'assistant', text: s.streamingText, tools });
      }
      if (extraError !== undefined) {
        items.push({ role: 'assistant', text: extraError, error: true });
      }
      history = [...history, ...exchangeToHistory(s.streamingText, tools)];
      return {
        ...s,
        viewItems: items,
        streaming: false,
        streamingText: '',
        reasoningText: '',
        toolRuns: [],
        taskId: null,
      };
    });
  };

  return {
    viewItems: [],
    streaming: false,
    streamingText: '',
    reasoningText: '',
    toolRuns: [],
    taskId: null,
    tools: [],
    servers: [],
    serverRunning: false,
    serverPort: null,
    panelVisible: false,

    setPanelVisible: (v) => set({ panelVisible: v }),

    send: async (text) => {
      const { streaming, viewItems } = get();
      if (streaming || !text.trim()) return;

      const ai = useSettingsStore.getState().settings.ai;
      history = [...history, { role: 'user', content: text }];

      set({
        viewItems: [...viewItems, { role: 'user', text }],
        streaming: true,
        streamingText: '',
        reasoningText: '',
        toolRuns: [],
      });

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
            finishTurn();
            break;
          case 'cancelled':
            finishTurn();
            break;
          case 'error':
            finishTurn(event.message);
            break;
        }
      };

      try {
        const taskId = await api.aiChatSend(
          providerConfigFromSettings(),
          ai.systemPrompt.trim() || null,
          history,
          ai.maxToolRounds,
          ai.mcpToolsEnabled,
          channel
        );
        set({ taskId });
      } catch (e) {
        // 配置错误等命令级失败: 无事件流, 直接呈现错误
        finishTurn(e instanceof Error ? e.message : String(e));
      }
    },

    cancel: async () => {
      const { taskId } = get();
      if (taskId) await api.aiChatCancel(taskId).catch(() => false);
    },

    clear: () => {
      history = [];
      set({ viewItems: [], streamingText: '', reasoningText: '', toolRuns: [], taskId: null, streaming: false });
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
