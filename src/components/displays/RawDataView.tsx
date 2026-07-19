import { useState, useEffect, useRef, useCallback } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { useAppStore } from '../../store/appStore';
import { rawDataBuffer } from '../../lib/dataBuffer';
import { useSelection } from '../../lib/useSelection';
import { writeTextToClipboard } from '../../lib/clipboard';
import { t } from '../../i18n';
import {
  Trash2,
  ArrowDown,
  Clock,
  Settings2,
  AlignLeft,
  PanelRight,
  Palette,
  Copy,
  Check,
  X,
  FileWarning,
} from 'lucide-react';
import { AppendMode, SendPanelMode, HexColorMode, ROW_HEIGHT, HeaderBytes, byteToHex, byteToAscii, formatTime } from './rawDataViewHelpers';
import { Row } from './RawDataRow';



/// 原始数据显示 — HEX + ASCII 双视图，支持虚拟滚动、选中复制、时间戳、发送
export function RawDataView() {
  const lang = useAppStore((s) => s.lang);
  const clearData = useAppStore((s) => s.clearData);
  const sendText = useAppStore((s) => s.sendText);

  const [view, setView] = useState<'hex' | 'ascii'>('hex');
  const [autoScroll, setAutoScroll] = useState(true);
  const [showTimestamp, setShowTimestamp] = useState(true);
  const [showOffset, setShowOffset] = useState(true);
  const [appendMode, setAppendMode] = useState<AppendMode>('nl');
  const [sendPanelMode, setSendPanelMode] = useState<SendPanelMode>('bottom');
  const [hexColorMode, setHexColorMode] = useState<HexColorMode>('printable');
  const [showSettings, setShowSettings] = useState(false);
  const [sendContent, setSendContent] = useState('');
  const [copyFeedback, setCopyFeedback] = useState(false);

  // 强制重新渲染的版本号 (RAF 节流后递增)
  const [version, setVersion] = useState(0);
  useEffect(() => {
    return rawDataBuffer.subscribe(() => setVersion((v) => v + 1));
  }, []);

  const lineCount = rawDataBuffer.lineCount;
  const totalBytes = rawDataBuffer.totalBytes;
  const droppedBytes = rawDataBuffer.droppedBytes;

  const parentRef = useRef<HTMLDivElement>(null);
  const userScrolledRef = useRef(false);
  const isAutoScrollingRef = useRef(false);

  const virtualizer = useVirtualizer({
    count: lineCount,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 10,
  });

  const selection = useSelection(lineCount);

  // 自动滚动
  useEffect(() => {
    if (!autoScroll || userScrolledRef.current || lineCount === 0) return;
    isAutoScrollingRef.current = true;
    virtualizer.scrollToIndex(lineCount - 1, { align: 'end' });
    // 短暂忽略本次滚动事件, 避免误判为用户滚动
    const t = setTimeout(() => {
      isAutoScrollingRef.current = false;
    }, 50);
    return () => clearTimeout(t);
  }, [lineCount, autoScroll, version, virtualizer]);

  // 检测用户手动滚动
  const handleScroll = useCallback(() => {
    if (isAutoScrollingRef.current || !parentRef.current) return;
    const el = parentRef.current;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 30;
    userScrolledRef.current = !atBottom;
  }, []);

  const handleClear = () => {
    clearData();
    selection.clear();
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

  const copySelected = useCallback(async () => {
    const indices = selection.selectedSorted;
    if (indices.length === 0) return;
    const lines = indices.map((i) => rawDataBuffer.getLine(i));
    const text = lines
      .map((line) => {
        const hex = Array.from(line.bytes, (b) => byteToHex(b)).join(' ');
        const ascii = Array.from(line.bytes, (b) => byteToAscii(b)).join('');
        return `${formatTime(line.timestamp)} ${line.offset.toString(16).padStart(8, '0').toUpperCase()}  ${hex.padEnd(48, ' ')}  |${ascii}|`;
      })
      .join('\n');
    const ok = await writeTextToClipboard(text);
    if (ok) {
      setCopyFeedback(true);
      setTimeout(() => setCopyFeedback(false), 1200);
    }
  }, [selection.selectedSorted]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      const target = e.target as HTMLElement;
      if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable) return;
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'a') {
        e.preventDefault();
        selection.selectAll();
      }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'c') {
        e.preventDefault();
        void copySelected();
      }
    },
    [selection, copySelected]
  );

  const handleRowMouseDown = useCallback(
    (e: React.MouseEvent, index: number) => {
      // 仅左键参与选择; 中键/右键不拦截
      if (e.button !== 0) return;
      selection.handleClick(index, e);
    },
    [selection]
  );

  const appendOptions: { mode: AppendMode; label: string }[] = [
    { mode: 'none', label: t(lang, 'appendNone') },
    { mode: 'nl', label: t(lang, 'appendNewline') },
    { mode: 'tab', label: t(lang, 'appendTab') },
    { mode: 'nl_tab', label: t(lang, 'appendNewlineTab') },
  ];

  const hexColorOptions: { mode: HexColorMode; label: string }[] = [
    { mode: 'none', label: t(lang, 'hexColorNone') },
    { mode: 'printable', label: t(lang, 'hexColorPrintable') },
    { mode: 'range', label: t(lang, 'hexColorRange') },
  ];

  const sendPanelOptions: { mode: SendPanelMode; label: string }[] = [
    { mode: 'bottom', label: t(lang, 'sendPanelBottom') },
    { mode: 'separate', label: t(lang, 'sendPanelSeparate') },
  ];

  const virtualItems = virtualizer.getVirtualItems();

  const renderHeader = () => (
    <div className="flex items-center gap-2 px-2 py-1 border-b border-border bg-bg-panel-header select-none h-[24px] flex-shrink-0">
      {showTimestamp && (
        <span className="text-text-secondary text-xs font-mono min-w-[92px] text-right">
          {t(lang, 'showTimestamp')}
        </span>
      )}
      {showOffset && (
        <span className="text-text-secondary text-xs font-mono min-w-[80px] text-right">Offset</span>
      )}
      {view === 'hex' ? (
        <>
          <div className="flex-1 flex gap-0.5">
            <HeaderBytes width={22} />
          </div>
          <div className="flex gap-0.5">
            <HeaderBytes width={18} />
          </div>
        </>
      ) : (
        <div className="flex gap-0.5">
          <HeaderBytes width={18} />
        </div>
      )}
    </div>
  );

  const renderContent = () => (
    <div className="flex-1 flex flex-col min-h-0 overflow-hidden font-mono">
      {renderHeader()}
      <div
        className="flex-1 overflow-auto min-h-0 outline-none"
        ref={parentRef}
        onScroll={handleScroll}
        onKeyDown={handleKeyDown}
        tabIndex={0}
      >
        {lineCount === 0 ? (
          <div className="flex items-center justify-center h-32 text-text-secondary text-sm">
            {t(lang, 'rawDataEmpty')}
          </div>
        ) : (
          <div style={{ height: `${virtualizer.getTotalSize()}px`, position: 'relative' }}>
            <div style={{ position: 'absolute', top: 0, left: 0, width: '100%', transform: `translateY(${virtualItems[0]?.start ?? 0}px)` }}>
              {virtualItems.map((virtualRow) => (
                <Row
                  key={virtualRow.key}
                  index={virtualRow.index}
                  view={view}
                  showTimestamp={showTimestamp}
                  showOffset={showOffset}
                  hexColorMode={hexColorMode}
                  isSelected={selection.isSelected(virtualRow.index)}
                  version={version}
                  onMouseDown={handleRowMouseDown}
                />
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );

  const renderAppendOptions = (vertical = false) => (
    <div className={`flex ${vertical ? 'flex-col' : 'items-center'} gap-0.5 ${vertical ? '' : 'flex-shrink-0'}`}>
      {!vertical && <span className="text-xs text-text-secondary mr-0.5">{t(lang, 'appendSuffix')}:</span>}
      {appendOptions.map((opt) => (
        <button
          key={opt.mode}
          className={`px-1.5 py-0.5 bg-bg-input border border-border rounded-sm text-text-secondary text-xs font-mono cursor-pointer transition-all hover:border-accent hover:text-text-primary ${appendMode === opt.mode ? 'bg-accent border-accent text-text-inverse' : ''}`}
          onClick={() => setAppendMode(opt.mode)}
        >
          {opt.label}
        </button>
      ))}
    </div>
  );

  const renderSendInput = () => (
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
  );

  const renderSendButton = () => (
    <button
      className="px-3 py-1.5 bg-bg-button text-text-inverse border-none rounded cursor-pointer text-sm text-center transition-colors hover:bg-bg-button-hover"
      onClick={handleSend}
    >
      {t(lang, 'send')}
    </button>
  );

  const renderSendPanel = () => (
    <div className="flex gap-1.5 p-1.5 items-center border-t border-border bg-bg-panel-header flex-shrink-0">
      {renderAppendOptions()}
      {renderSendInput()}
      {renderSendButton()}
    </div>
  );

  const renderSendPanelCompact = () => (
    <div className="flex flex-col gap-1.5">
      <span className="text-xs text-text-secondary">{t(lang, 'appendSuffix')}</span>
      {renderAppendOptions(true)}
      {renderSendInput()}
      {renderSendButton()}
    </div>
  );

  const renderSettingsPanelContent = () => (
    <div className="flex flex-col gap-4">
      <div>
        <h4 className="text-xs font-semibold text-text-secondary mb-2 flex items-center gap-1">
          <Palette size={12} /> {t(lang, 'hexColorMode')}
        </h4>
        <div className="flex flex-col gap-1">
          {hexColorOptions.map((opt) => (
            <button
              key={opt.mode}
              className={`text-left px-2 py-1 rounded text-xs transition-colors ${hexColorMode === opt.mode ? 'bg-bg-active text-text-bright' : 'text-text-secondary hover:bg-bg-hover hover:text-text-primary'}`}
              onClick={() => setHexColorMode(opt.mode)}
            >
              {opt.label}
            </button>
          ))}
        </div>
      </div>
      <div>
        <h4 className="text-xs font-semibold text-text-secondary mb-2 flex items-center gap-1">
          <PanelRight size={12} /> {t(lang, 'sendPanelMode')}
        </h4>
        <div className="flex flex-col gap-1">
          {sendPanelOptions.map((opt) => (
            <button
              key={opt.mode}
              className={`text-left px-2 py-1 rounded text-xs transition-colors ${sendPanelMode === opt.mode ? 'bg-bg-active text-text-bright' : 'text-text-secondary hover:bg-bg-hover hover:text-text-primary'}`}
              onClick={() => setSendPanelMode(opt.mode)}
            >
              {opt.label}
            </button>
          ))}
        </div>
      </div>
      <div>
        <h4 className="text-xs font-semibold text-text-secondary mb-2 flex items-center gap-1">
          <AlignLeft size={12} /> {t(lang, 'displayOptions')}
        </h4>
        <label className="flex items-center gap-2 text-xs text-text-secondary hover:text-text-primary cursor-pointer mb-1.5">
          <input
            type="checkbox"
            checked={showTimestamp}
            onChange={(e) => setShowTimestamp(e.target.checked)}
            className="accent-accent"
          />
          {t(lang, 'showTimestamp')}
        </label>
        <label className="flex items-center gap-2 text-xs text-text-secondary hover:text-text-primary cursor-pointer">
          <input
            type="checkbox"
            checked={showOffset}
            onChange={(e) => setShowOffset(e.target.checked)}
            className="accent-accent"
          />
          {t(lang, 'showOffset')}
        </label>
      </div>
    </div>
  );

  return (
    <div className="h-full flex flex-col overflow-hidden">
      <div className="flex gap-1 p-1.5 items-center border-b border-border bg-bg-panel-header flex-shrink-0">
        <div className="flex items-center bg-bg-input rounded p-0.5">
          <button
            className={`px-2 py-0.5 rounded text-xs font-medium transition-colors cursor-pointer ${view === 'hex' ? 'bg-bg-button text-text-inverse' : 'text-text-secondary hover:text-text-primary'}`}
            onClick={() => setView('hex')}
          >
            {t(lang, 'hexView')}
          </button>
          <button
            className={`px-2 py-0.5 rounded text-xs font-medium transition-colors cursor-pointer ${view === 'ascii' ? 'bg-bg-button text-text-inverse' : 'text-text-secondary hover:text-text-primary'}`}
            onClick={() => setView('ascii')}
          >
            {t(lang, 'asciiView')}
          </button>
        </div>

        <div className="flex items-center gap-1 text-text-secondary text-xs font-mono">
          <span>{totalBytes.toLocaleString()} B</span>
          {droppedBytes > 0 && (
            <span className="text-red flex items-center gap-0.5" title={t(lang, 'rawDataDropped')}>
              <FileWarning size={12} />
              +{droppedBytes.toLocaleString()}
            </span>
          )}
        </div>

        <div className="flex-1" />

        {selection.selected.size > 0 && (
          <>
            <span className="text-text-secondary text-xs">{selection.selected.size}</span>
            <button
              className={`w-7 h-7 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer ${copyFeedback ? 'text-green' : ''}`}
              title={t(lang, 'copySelected')}
              onClick={() => void copySelected()}
            >
              {copyFeedback ? <Check size={14} /> : <Copy size={14} />}
            </button>
            <button
              className="w-7 h-7 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer"
              title={t(lang, 'clearSelection')}
              onClick={selection.clear}
            >
              <X size={14} />
            </button>
          </>
        )}

        <button
          className={`w-7 h-7 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer ${showTimestamp ? 'text-text-bright bg-bg-hover' : ''}`}
          title={t(lang, 'showTimestamp')}
          onClick={() => setShowTimestamp(!showTimestamp)}
        >
          <Clock size={14} />
        </button>
        <button
          className={`w-7 h-7 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer ${autoScroll && !userScrolledRef.current ? 'text-text-bright bg-bg-hover' : ''}`}
          title={t(lang, 'autoScroll')}
          onClick={() => {
            setAutoScroll(!autoScroll);
            userScrolledRef.current = false;
          }}
        >
          <ArrowDown size={14} />
        </button>
        <button
          className={`w-7 h-7 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer ${showSettings ? 'text-text-bright bg-bg-hover' : ''}`}
          title={t(lang, 'settings')}
          onClick={() => setShowSettings(!showSettings)}
        >
          <Settings2 size={14} />
        </button>
        <button
          className="w-7 h-7 flex items-center justify-center rounded text-text-secondary hover:bg-bg-danger hover:text-text-bright transition-colors cursor-pointer"
          title={t(lang, 'clear')}
          onClick={handleClear}
        >
          <Trash2 size={14} />
        </button>
      </div>
      <div className="flex-1 flex overflow-hidden min-h-0">
        {sendPanelMode === 'separate' ? (
          <>
            <div className="flex-1 flex flex-col min-w-0 overflow-hidden">
              {renderContent()}
            </div>
            <div className="w-[220px] flex-shrink-0 border-l border-border bg-bg-sidebar flex flex-col overflow-hidden">
              {showSettings ? (
                <div className="flex-1 overflow-y-auto p-3">
                  {renderSettingsPanelContent()}
                </div>
              ) : (
                <div className="flex-1" />
              )}
              <div className="border-t border-border p-2 flex flex-col gap-1.5">
                {renderSendPanelCompact()}
              </div>
            </div>
          </>
        ) : (
          <div className="flex-1 flex flex-col min-w-0 overflow-hidden">
            {renderContent()}
            {showSettings && (
              <div className="border-t border-border p-3 bg-bg-sidebar overflow-y-auto max-h-[180px]">
                {renderSettingsPanelContent()}
              </div>
            )}
            {renderSendPanel()}
          </div>
        )}
      </div>
    </div>
  );
}
