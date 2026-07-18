import { useState, useEffect, useRef, useCallback } from 'react';
import { useAppStore } from '../../store/appStore';
import { rawDataBuffer, type RawDataSnapshot } from '../../lib/dataBuffer';
import { t } from '../../i18n';
import { Trash2, ArrowDown, Clock } from 'lucide-react';

type AppendMode = 'none' | 'nl' | 'tab' | 'nl_tab';

/// 格式化时间戳为 HH:MM:SS.mmm
function formatTime(ts: number): string {
  if (!ts) return '--:--:--.---';
  const d = new Date(ts);
  const h = d.getHours().toString().padStart(2, '0');
  const m = d.getMinutes().toString().padStart(2, '0');
  const s = d.getSeconds().toString().padStart(2, '0');
  const ms = d.getMilliseconds().toString().padStart(3, '0');
  return `${h}:${m}:${s}.${ms}`;
}

/// 原始数据显示 — HEX + ASCII 双视图, 支持时间戳, 发送附加选项
export function RawDataView() {
  const lang = useAppStore((s) => s.lang);
  const clearData = useAppStore((s) => s.clearData);
  const sendText = useAppStore((s) => s.sendText);
  const rawDataVersion = useAppStore((s) => s.rawDataVersion);

  const [view, setView] = useState<'hex' | 'ascii'>('hex');
  const [autoScroll, setAutoScroll] = useState(true);
  const [showTimestamp, setShowTimestamp] = useState(true);
  const [appendMode, setAppendMode] = useState<AppendMode>('nl');
  const [snapshot, setSnapshot] = useState<RawDataSnapshot>({
    hex: '',
    ascii: '',
    timestamps: [],
    offset: 0,
  });
  const [sendContent, setSendContent] = useState('');
  const contentRef = useRef<HTMLDivElement>(null);
  const userScrolledRef = useRef(false);

  useEffect(() => {
    const update = (data: RawDataSnapshot) => {
      setSnapshot(data);
    };
    const unsub = rawDataBuffer.subscribe(update);
    const initial = rawDataBuffer.getRecentLines();
    setSnapshot(initial);
    return unsub;
  }, []);

  // 自动滚动: 只在 autoScroll 且用户未手动滚动时执行
  useEffect(() => {
    if (autoScroll && !userScrolledRef.current && contentRef.current) {
      contentRef.current.scrollTop = contentRef.current.scrollHeight;
    }
  }, [snapshot, autoScroll, rawDataVersion]);

  // 检测用户手动滚动
  const handleScroll = useCallback(() => {
    if (!contentRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = contentRef.current;
    const atBottom = scrollHeight - scrollTop - clientHeight < 30;
    if (!atBottom) {
      // 用户向上滚动, 标记为手动滚动
      userScrolledRef.current = true;
    } else {
      // 滚到底部, 清除手动滚动标记
      userScrolledRef.current = false;
    }
  }, []);

  const handleClear = () => {
    clearData();
    setSnapshot({ hex: '', ascii: '', timestamps: [], offset: 0 });
    userScrolledRef.current = false;
  };

  const handleSend = () => {
    if (!sendContent) return;
    let suffix = '';
    switch (appendMode) {
      case 'nl': suffix = '\n'; break;
      case 'tab': suffix = '\t'; break;
      case 'nl_tab': suffix = '\n\t'; break;
      case 'none': suffix = ''; break;
    }
    sendText(sendContent + suffix);
    setSendContent('');
  };

  const formatHexView = () => {
    const lines = snapshot.hex.split('\n');
    const asciiLines = snapshot.ascii.split('\n');
    return lines.map((hexLine, i) => {
      const lineOffset = snapshot.offset + i * 16;
      const ts = snapshot.timestamps[i] ?? 0;
      return (
        <div key={i} className="flex gap-3 whitespace-pre">
          {showTimestamp && (
            <span className="text-accent min-w-[100px] text-xs">{formatTime(ts)}</span>
          )}
          <span className="text-text-secondary min-w-[72px]">
            {lineOffset.toString(16).padStart(8, '0')}
          </span>
          <span className="text-text-primary min-w-[360px]">{hexLine}</span>
          <span className="text-green">{asciiLines[i] || ''}</span>
        </div>
      );
    });
  };

  const appendOptions: { mode: AppendMode; label: string }[] = [
    { mode: 'none', label: t(lang, 'appendNone') },
    { mode: 'nl', label: t(lang, 'appendNewline') },
    { mode: 'tab', label: t(lang, 'appendTab') },
    { mode: 'nl_tab', label: t(lang, 'appendNewlineTab') },
  ];

  return (
    <div className="h-full flex flex-col overflow-hidden">
      <div className="flex gap-1 p-1 items-center border-b border-border bg-bg-panel-header flex-shrink-0">
        <button
          className={`w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer ${view === 'hex' ? 'text-text-bright' : ''}`}
          onClick={() => setView('hex')}
        >
          {t(lang, 'hexView')}
        </button>
        <button
          className={`w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer ${view === 'ascii' ? 'text-text-bright' : ''}`}
          onClick={() => setView('ascii')}
        >
          {t(lang, 'asciiView')}
        </button>
        <div className="flex-1" />
        <button
          className={`w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer ${showTimestamp ? 'text-text-bright' : ''}`}
          title={t(lang, 'showTimestamp')}
          onClick={() => setShowTimestamp(!showTimestamp)}
        >
          <Clock size={14} />
        </button>
        <button
          className={`w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer ${autoScroll && !userScrolledRef.current ? 'text-text-bright' : ''}`}
          title={t(lang, 'autoScroll')}
          onClick={() => {
            setAutoScroll(!autoScroll);
            userScrolledRef.current = false;
          }}
        >
          <ArrowDown size={14} />
        </button>
        <button
          className="w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer"
          title={t(lang, 'clear')}
          onClick={handleClear}
        >
          <Trash2 size={14} />
        </button>
      </div>
      <div
        className="flex-1 overflow-y-auto overflow-x-hidden font-mono text-sm px-2 py-1 leading-relaxed min-h-0"
        ref={contentRef}
        onScroll={handleScroll}
      >
        {view === 'hex' ? (
          formatHexView()
        ) : (
          <div className="font-mono">
            {snapshot.ascii.split('\n').map((line, i) => (
              <div key={i} className="flex gap-3 whitespace-pre">
                {showTimestamp && (
                  <span className="text-accent min-w-[100px] text-xs">
                    {formatTime(snapshot.timestamps[i] ?? 0)}
                  </span>
                )}
                <span className="text-green">{line}</span>
              </div>
            ))}
          </div>
        )}
      </div>
      <div className="flex gap-1 p-1 items-center border-t border-border bg-bg-panel-header flex-shrink-0">
        <div className="flex items-center gap-0.5 flex-shrink-0">
          <span className="text-xs text-text-secondary mr-0.5">{t(lang, 'appendSuffix')}:</span>
          {appendOptions.map((opt) => (
            <button
              key={opt.mode}
              className={`px-1.5 py-0.5 bg-bg-input border border-border rounded-sm text-text-secondary text-xs font-mono cursor-pointer transition-all hover:border-accent hover:text-text-primary ${appendMode === opt.mode ? 'bg-accent border-accent text-text-bright' : ''}`}
              onClick={() => setAppendMode(opt.mode)}
            >
              {opt.label}
            </button>
          ))}
        </div>
        <input
          type="text"
          className="flex-1 min-w-[60px] px-2 py-1 bg-bg-input text-text-primary border border-border rounded text-sm focus:outline-none focus:border-accent transition-colors"
          placeholder={lang === 'zh' ? '输入要发送的文本...' : 'Type to send...'}
          value={sendContent}
          onChange={(e) => setSendContent(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') handleSend();
          }}
        />
        <button className="px-3 py-1.5 bg-bg-button text-text-bright border-none rounded cursor-pointer text-sm text-center transition-colors hover:bg-bg-button-hover" onClick={handleSend}>
          {t(lang, 'send')}
        </button>
      </div>
    </div>
  );
}
