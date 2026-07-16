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
        <div key={i} className="raw-data-line">
          {showTimestamp && (
            <span className="raw-data-time">{formatTime(ts)}</span>
          )}
          <span className="raw-data-offset">
            {lineOffset.toString(16).padStart(8, '0')}
          </span>
          <span className="raw-data-hex">{hexLine}</span>
          <span className="raw-data-ascii">{asciiLines[i] || ''}</span>
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
    <div className="raw-data-view">
      <div className="raw-data-toolbar">
        <button
          className={`btn-icon ${view === 'hex' ? 'active' : ''}`}
          style={view === 'hex' ? { color: 'var(--text-bright)' } : {}}
          onClick={() => setView('hex')}
        >
          {t(lang, 'hexView')}
        </button>
        <button
          className={`btn-icon ${view === 'ascii' ? 'active' : ''}`}
          style={view === 'ascii' ? { color: 'var(--text-bright)' } : {}}
          onClick={() => setView('ascii')}
        >
          {t(lang, 'asciiView')}
        </button>
        <div style={{ flex: 1 }} />
        <button
          className={`btn-icon ${showTimestamp ? 'active' : ''}`}
          style={showTimestamp ? { color: 'var(--text-bright)' } : {}}
          title={t(lang, 'showTimestamp')}
          onClick={() => setShowTimestamp(!showTimestamp)}
        >
          <Clock size={14} />
        </button>
        <button
          className={`btn-icon ${autoScroll && !userScrolledRef.current ? 'active' : ''}`}
          style={autoScroll && !userScrolledRef.current ? { color: 'var(--text-bright)' } : {}}
          title={t(lang, 'autoScroll')}
          onClick={() => {
            setAutoScroll(!autoScroll);
            userScrolledRef.current = false;
          }}
        >
          <ArrowDown size={14} />
        </button>
        <button
          className="btn-icon"
          title={t(lang, 'clear')}
          onClick={handleClear}
        >
          <Trash2 size={14} />
        </button>
      </div>
      <div
        className="raw-data-content"
        ref={contentRef}
        onScroll={handleScroll}
      >
        {view === 'hex' ? (
          formatHexView()
        ) : (
          <div className="raw-data-ascii-text">
            {snapshot.ascii.split('\n').map((line, i) => (
              <div key={i} className="raw-data-line">
                {showTimestamp && (
                  <span className="raw-data-time">
                    {formatTime(snapshot.timestamps[i] ?? 0)}
                  </span>
                )}
                <span className="raw-data-ascii">{line}</span>
              </div>
            ))}
          </div>
        )}
      </div>
      <div className="send-bar">
        <div className="append-selector">
          <span className="append-label">{t(lang, 'appendSuffix')}:</span>
          {appendOptions.map((opt) => (
            <button
              key={opt.mode}
              className={`append-btn ${appendMode === opt.mode ? 'active' : ''}`}
              onClick={() => setAppendMode(opt.mode)}
            >
              {opt.label}
            </button>
          ))}
        </div>
        <input
          type="text"
          className="send-input"
          placeholder={lang === 'zh' ? '输入要发送的文本...' : 'Type to send...'}
          value={sendContent}
          onChange={(e) => setSendContent(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') handleSend();
          }}
        />
        <button className="btn" onClick={handleSend}>
          {t(lang, 'send')}
        </button>
      </div>
    </div>
  );
}
