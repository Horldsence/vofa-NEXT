import { useEffect, useRef, useState } from 'react';
import {
  Bot,
  ChevronDown,
  Eraser,
  Plug,
  Plus,
  Send,
  Square,
  Trash2,
  Wrench,
  X,
} from 'lucide-react';
import { useAppStore } from '../../../store/appStore';
import { useAiChatStore } from '../../../store/aiChatStore';
import { t } from '../../../i18n';
import type { AiToolRun } from '../../../types';

/// 工具名缩短展示 (去掉 mcp_ 前缀与 server 段)
function shortToolName(name: string): string {
  return name.replace(/^mcp_[^_]+_/, '');
}

/// 工具调用卡片 (运行中 / 完成 / 失败)
function ToolRunCard({ run }: { run: AiToolRun }) {
  const lang = useAppStore((s) => s.lang);
  const [open, setOpen] = useState(false);
  return (
    <div
      className={`rounded border text-[11px] ${
        run.is_error
          ? 'border-danger/40 bg-danger/10'
          : run.done
            ? 'border-border-subtle bg-bg-hover/40'
            : 'border-accent/40 bg-accent/10'
      }`}
    >
      <button
        className="w-full flex items-center gap-1.5 px-2 py-1 text-left"
        onClick={() => setOpen((v) => !v)}
      >
        <Wrench size={11} className="shrink-0 text-text-secondary" />
        <span className="font-medium truncate">{shortToolName(run.name)}</span>
        <span className="ml-auto text-text-secondary shrink-0">
          {!run.done ? t(lang, 'aiToolRunning') : run.is_error ? t(lang, 'aiToolFailed') : t(lang, 'aiToolDone')}
        </span>
        <ChevronDown size={11} className={`shrink-0 transition-transform ${open ? 'rotate-180' : ''}`} />
      </button>
      {open && (
        <div className="px-2 pb-1.5 space-y-1 border-t border-border-subtle pt-1.5">
          <pre className="whitespace-pre-wrap break-all text-text-secondary max-h-24 overflow-y-auto">
            {JSON.stringify(run.arguments, null, 2)}
          </pre>
          {run.content && (
            <pre className="whitespace-pre-wrap break-all max-h-40 overflow-y-auto">{run.content}</pre>
          )}
        </div>
      )}
    </div>
  );
}

/// 外部 MCP server 管理抽屉
function McpDrawer({ onClose }: { onClose: () => void }) {
  const lang = useAppStore((s) => s.lang);
  const { servers, tools, addServer, removeServer, setServerEnabled, refreshTools } = useAiChatStore();
  const [name, setName] = useState('');
  const [command, setCommand] = useState('');
  const [url, setUrl] = useState('');
  const [kind, setKind] = useState<'stdio' | 'http'>('stdio');

  const canAdd =
    name.trim().length > 0 &&
    (kind === 'stdio' ? command.trim().length > 0 : url.startsWith('http://') || url.startsWith('https://'));

  const onAdd = async () => {
    await addServer({
      id: `srv-${Date.now().toString(36)}`,
      name: name.trim(),
      transport:
        kind === 'stdio'
          ? { kind: 'stdio', command: command.trim(), args: command.trim().split(/\s+/).slice(1), env: {} }
          : { kind: 'http', url: url.trim() },
      enabled: true,
    });
    setName('');
    setCommand('');
    setUrl('');
    await refreshTools();
  };

  return (
    <div className="absolute inset-0 z-10 bg-bg-panel/95 backdrop-blur-sm flex flex-col">
      <div className="flex items-center gap-2 px-3 h-9 border-b border-border-subtle">
        <span className="text-xs font-medium">{t(lang, 'aiMcpServers')}</span>
        <span className="text-[11px] text-text-secondary">{t(lang, 'aiMcpServersHint')}</span>
        <button className="ml-auto p-1 rounded hover:bg-bg-hover" onClick={onClose} title={t(lang, 'aiClose')}>
          <X size={13} />
        </button>
      </div>
      <div className="flex-1 overflow-y-auto px-3 py-2 space-y-2 text-xs">
        {servers.length === 0 && <div className="text-text-secondary">{t(lang, 'aiNoServers')}</div>}
        {servers.map((srv) => (
          <div key={srv.id} className="flex items-center gap-2 rounded border border-border-subtle px-2 py-1.5">
            <input
              type="checkbox"
              checked={srv.enabled}
              onChange={(e) => setServerEnabled(srv.id, e.target.checked)}
              className="accent-accent"
            />
            <span className="font-medium">{srv.name}</span>
            <span className="text-text-secondary truncate">
              {srv.transport.kind === 'stdio'
                ? `${srv.transport.command} ${srv.transport.args.join(' ')}`
                : srv.transport.url}
            </span>
            <button
              className="ml-auto p-1 rounded text-text-secondary hover:bg-bg-hover hover:text-danger"
              onClick={() => removeServer(srv.id)}
              title={t(lang, 'aiDeleteServer')}
            >
              <Trash2 size={12} />
            </button>
          </div>
        ))}

        <div className="rounded border border-border-subtle px-2 py-1.5 space-y-1.5">
          <div className="font-medium text-text-secondary">{t(lang, 'aiAddServer')}</div>
          <div className="flex gap-1.5">
            <select
              value={kind}
              onChange={(e) => setKind(e.target.value as 'stdio' | 'http')}
              className="bg-bg-hover rounded px-1 py-0.5 outline-none"
            >
              <option value="stdio">stdio</option>
              <option value="http">http</option>
            </select>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={t(lang, 'aiServerName')}
              className="flex-1 min-w-0 bg-bg-hover rounded px-1.5 py-0.5 outline-none focus:ring-1 ring-accent"
            />
          </div>
          {kind === 'stdio' ? (
            <input
              value={command}
              onChange={(e) => setCommand(e.target.value)}
              placeholder={t(lang, 'aiServerCommand')}
              className="w-full bg-bg-hover rounded px-1.5 py-0.5 outline-none focus:ring-1 ring-accent"
            />
          ) : (
            <input
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="http://127.0.0.1:8000/mcp"
              className="w-full bg-bg-hover rounded px-1.5 py-0.5 outline-none focus:ring-1 ring-accent"
            />
          )}
          <button
            className="px-2 py-0.5 rounded bg-accent text-accent-foreground disabled:opacity-40 flex items-center gap-1"
            disabled={!canAdd}
            onClick={onAdd}
          >
            <Plus size={11} />
            {t(lang, 'aiAddServer')}
          </button>
        </div>

        {tools.length > 0 && (
          <div className="pt-1">
            <div className="font-medium text-text-secondary pb-1">
              {t(lang, 'aiToolCount').replace('{n}', String(tools.length))}
            </div>
            <div className="flex flex-wrap gap-1">
              {tools.map((tool) => (
                <span
                  key={`${tool.server_id}-${tool.name}`}
                  title={tool.description}
                  className="px-1.5 py-0.5 rounded bg-bg-hover text-[10px]"
                >
                  {tool.prefixed_name}
                </span>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

/// AI 对话底部面板 — 流式对话 + MCP 工具调用展示 + 本地 MCP server 管理
export function AiChatPanel() {
  const lang = useAppStore((s) => s.lang);
  const {
    viewItems,
    streaming,
    streamingText,
    reasoningText,
    toolRuns,
    tools,
    serverRunning,
    serverPort,
    send,
    cancel,
    clear,
    setPanelVisible,
    refreshTools,
    refreshServerStatus,
    startLocalServer,
  } = useAiChatStore();
  const [input, setInput] = useState('');
  const [drawerOpen, setDrawerOpen] = useState(false);
  const listRef = useRef<HTMLDivElement>(null);
  const pinnedBottom = useRef(true);

  // 打开面板: 拉取工具/状态, 并按需自启本地 MCP server (opt-in 语义)
  useEffect(() => {
    refreshServerStatus();
    refreshTools();
    if (!useAiChatStore.getState().serverRunning) {
      startLocalServer();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 用户未上滚时自动跟随最新消息
  useEffect(() => {
    const el = listRef.current;
    if (el && pinnedBottom.current) {
      el.scrollTop = el.scrollHeight;
    }
  }, [viewItems, streamingText, reasoningText, toolRuns]);

  const onSend = () => {
    const text = input.trim();
    if (!text || streaming) return;
    setInput('');
    void send(text);
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault();
      onSend();
    }
  };

  return (
    <div className="relative h-full flex flex-col overflow-hidden">
      {/* 标题栏 */}
      <div className="flex items-center gap-2 px-3 h-8 border-b border-border-subtle shrink-0">
        <Bot size={13} className="text-accent" />
        <span className="text-xs font-medium">{t(lang, 'aiChat')}</span>
        <button
          className="flex items-center gap-1 px-1.5 h-5 rounded text-[11px] text-text-secondary hover:bg-bg-hover hover:text-text-primary"
          onClick={() => setDrawerOpen(true)}
          title={t(lang, 'aiMcpServers')}
        >
          <Plug size={11} />
          {t(lang, 'aiToolCount').replace('{n}', String(tools.length))}
        </button>
        <span
          className={`text-[10px] px-1.5 h-4 flex items-center rounded ${
            serverRunning ? 'bg-success/15 text-success' : 'bg-bg-hover text-text-secondary'
          }`}
          title={serverRunning ? `127.0.0.1:${serverPort ?? ''}/mcp` : t(lang, 'aiServerStopped')}
        >
          {serverRunning ? `MCP :${serverPort ?? ''}` : t(lang, 'aiServerStopped')}
        </span>
        <div className="ml-auto flex items-center gap-0.5">
          <button
            className="p-1 rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary disabled:opacity-40"
            onClick={clear}
            disabled={streaming || viewItems.length === 0}
            title={t(lang, 'aiClear')}
          >
            <Eraser size={12} />
          </button>
          <button
            className="p-1 rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary"
            onClick={() => setPanelVisible(false)}
            title={t(lang, 'aiClose')}
          >
            <X size={13} />
          </button>
        </div>
      </div>

      {/* 消息区 */}
      <div
        ref={listRef}
        className="flex-1 min-h-0 overflow-y-auto px-3 py-2 space-y-2 text-xs"
        onScroll={(e) => {
          const el = e.currentTarget;
          pinnedBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 32;
        }}
      >
        {viewItems.length === 0 && !streaming && (
          <div className="h-full flex items-center justify-center text-text-secondary">{t(lang, 'aiPlaceholder')}</div>
        )}
        {viewItems.map((item, i) => (
          <div key={i} className={`flex ${item.role === 'user' ? 'justify-end' : 'justify-start'}`}>
            <div
              className={`max-w-[85%] rounded-lg px-2.5 py-1.5 space-y-1.5 ${
                item.role === 'user'
                  ? 'bg-accent text-accent-foreground'
                  : item.error
                    ? 'bg-danger/10 text-danger border border-danger/30'
                    : 'bg-bg-hover'
              }`}
            >
              {item.tools && item.tools.length > 0 && (
                <div className="space-y-1">
                  {item.tools.map((run) => (
                    <ToolRunCard key={run.id} run={run} />
                  ))}
                </div>
              )}
              {item.text && <div className="whitespace-pre-wrap break-words">{item.text}</div>}
            </div>
          </div>
        ))}

        {/* 流式中的回合 */}
        {streaming && (
          <div className="flex justify-start">
            <div className="max-w-[85%] rounded-lg px-2.5 py-1.5 space-y-1.5 bg-bg-hover">
              {toolRuns.map((run) => (
                <ToolRunCard key={run.id} run={run} />
              ))}
              {reasoningText && (
                <div className="whitespace-pre-wrap break-words text-text-secondary/70 italic line-clamp-3">
                  {reasoningText}
                </div>
              )}
              <div className="whitespace-pre-wrap break-words">
                {streamingText}
                <span className="inline-block w-1.5 h-3 ml-0.5 align-text-bottom bg-accent animate-pulse" />
              </div>
            </div>
          </div>
        )}
      </div>

      {/* 输入区 */}
      <div className="flex items-end gap-2 px-3 py-2 border-t border-border-subtle shrink-0">
        <textarea
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={onKeyDown}
          placeholder={t(lang, 'inputMessage')}
          rows={1}
          className="flex-1 resize-none bg-bg-hover rounded px-2 py-1.5 text-xs outline-none focus:ring-1 ring-accent max-h-24"
        />
        {streaming ? (
          <button
            className="h-7 px-2.5 rounded bg-danger text-white text-xs flex items-center gap-1 shrink-0"
            onClick={() => void cancel()}
          >
            <Square size={11} />
            {t(lang, 'aiStop')}
          </button>
        ) : (
          <button
            className="h-7 px-2.5 rounded bg-accent text-accent-foreground text-xs flex items-center gap-1 shrink-0 disabled:opacity-40"
            onClick={onSend}
            disabled={!input.trim()}
          >
            <Send size={11} />
            {t(lang, 'aiSend')}
          </button>
        )}
      </div>

      {drawerOpen && <McpDrawer onClose={() => setDrawerOpen(false)} />}
    </div>
  );
}
