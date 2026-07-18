import { useState, useEffect, useRef, useCallback, useMemo, memo } from 'react';
import { useAppStore } from '../../store/appStore';
import { rawDataBuffer, type RawDataSnapshot, type RawDataLine } from '../../lib/dataBuffer';
import { t } from '../../i18n';
import { Trash2, ArrowDown, Clock, Settings2, AlignLeft, PanelRight, Palette } from 'lucide-react';

type AppendMode = 'none' | 'nl' | 'tab' | 'nl_tab';
type SendPanelMode = 'bottom' | 'separate';
type HexColorMode = 'none' | 'printable' | 'range';

const BYTES_PER_ROW = 16;
const GROUP_SIZE = 8;

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

/// 格式化单字节为 2 位大写 hex
function byteToHex(b: number): string {
  return b.toString(16).padStart(2, '0').toUpperCase();
}

/// 格式化单字节为 ascii（不可打印显示为 .）
function byteToAscii(b: number): string {
  return b >= 32 && b < 127 ? String.fromCharCode(b) : '.';
}

/// 判断字节是否为可打印 ASCII
function isPrintable(b: number): boolean {
  return b >= 32 && b < 127;
}

/// Hex 字节颜色类（根据颜色模式）
function hexColorClass(b: number, mode: HexColorMode): string {
  if (mode === 'none') return 'text-text-primary';
  if (mode === 'printable') {
    if (b === 0) return 'text-text-disabled';
    return isPrintable(b) ? 'text-text-primary' : 'text-text-secondary';
  }
  // range 模式
  if (b === 0x00) return 'text-text-disabled';
  if (b === 0xff) return 'text-red';
  if (isPrintable(b)) return 'text-blue';
  if (b < 0x20) return 'text-yellow';
  return 'text-text-secondary';
}

/// 比较两个 Uint8Array 风格数组内容是否相同
function bytesEqual(a: number[], b: number[]): boolean {
  if (a === b) return true;
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}

interface HexRowProps {
  line: RawDataLine;
  showTimestamp: boolean;
  showOffset: boolean;
  hexColorMode: HexColorMode;
}

/// Hex 视图行 — memo 化, 仅在字节内容或显示选项变化时重渲染
const HexRow = memo(function HexRow({ line, showTimestamp, showOffset, hexColorMode }: HexRowProps) {
  const [hovered, setHovered] = useState<number | null>(null);
  return (
    <div
      className="flex items-center gap-2 px-2 py-0.5 hover:bg-bg-hover transition-colors"
      onMouseLeave={() => setHovered(null)}
    >
      {showTimestamp && (
        <span className="text-accent text-xs font-mono min-w-[92px] text-right">{formatTime(line.timestamp)}</span>
      )}
      {showOffset && (
        <span className="text-text-secondary text-xs font-mono min-w-[72px] text-right">
          {line.offset.toString(16).padStart(8, '0').toUpperCase()}
        </span>
      )}
      <div className="flex-1 flex gap-0.5">
        {line.bytes.map((b, i) => {
          const isGroupEnd = (i + 1) % GROUP_SIZE === 0 && i !== BYTES_PER_ROW - 1;
          return (
            <span
              key={i}
              className={`
                inline-flex items-center justify-center font-mono text-xs
                w-[22px] h-[18px] rounded-sm cursor-default select-text
                transition-colors
                ${hexColorClass(b, hexColorMode)}
                ${hovered === i ? 'bg-bg-active text-text-bright' : ''}
                ${isGroupEnd ? 'mr-2' : ''}
              `}
              onMouseEnter={() => setHovered(i)}
            >
              {byteToHex(b)}
            </span>
          );
        })}
        {Array.from({ length: BYTES_PER_ROW - line.bytes.length }).map((_, i) => (
          <span key={`pad-${i}`} className="inline-flex w-[22px] h-[18px]" />
        ))}
      </div>
      <div className="flex gap-0.5">
        {line.bytes.map((b, i) => {
          const isGroupEnd = (i + 1) % GROUP_SIZE === 0 && i !== BYTES_PER_ROW - 1;
          const printable = isPrintable(b);
          return (
            <span
              key={i}
              className={`
                inline-flex items-center justify-center font-mono text-xs
                w-[18px] h-[18px] rounded-sm cursor-default select-text
                transition-colors
                ${printable ? 'text-green' : 'text-text-disabled'}
                ${hovered === i ? 'bg-bg-active text-text-bright' : ''}
                ${isGroupEnd ? 'mr-2' : ''}
              `}
              onMouseEnter={() => setHovered(i)}
            >
              {byteToAscii(b)}
            </span>
          );
        })}
      </div>
    </div>
  );
}, (prev, next) => {
  return (
    prev.showTimestamp === next.showTimestamp &&
    prev.showOffset === next.showOffset &&
    prev.hexColorMode === next.hexColorMode &&
    prev.line.offset === next.line.offset &&
    prev.line.timestamp === next.line.timestamp &&
    bytesEqual(prev.line.bytes, next.line.bytes)
  );
});

interface AsciiRowProps {
  line: RawDataLine;
  showTimestamp: boolean;
  showOffset: boolean;
}

/// ASCII 视图行 — memo 化
const AsciiRow = memo(function AsciiRow({ line, showTimestamp, showOffset }: AsciiRowProps) {
  const [hovered, setHovered] = useState<number | null>(null);
  return (
    <div
      className="flex items-center gap-2 px-2 py-0.5 hover:bg-bg-hover transition-colors"
      onMouseLeave={() => setHovered(null)}
    >
      {showTimestamp && (
        <span className="text-accent text-xs font-mono min-w-[92px] text-right">{formatTime(line.timestamp)}</span>
      )}
      {showOffset && (
        <span className="text-text-secondary text-xs font-mono min-w-[72px] text-right">
          {line.offset.toString(16).padStart(8, '0').toUpperCase()}
        </span>
      )}
      <div className="flex gap-0.5">
        {line.bytes.map((b, i) => {
          const isGroupEnd = (i + 1) % GROUP_SIZE === 0 && i !== BYTES_PER_ROW - 1;
          const printable = isPrintable(b);
          return (
            <span
              key={i}
              className={`
                inline-flex items-center justify-center font-mono text-xs
                w-[18px] h-[18px] rounded-sm cursor-default select-text
                transition-colors
                ${printable ? 'text-green' : 'text-text-disabled'}
                ${hovered === i ? 'bg-bg-active text-text-bright' : ''}
                ${isGroupEnd ? 'mr-2' : ''}
              `}
              onMouseEnter={() => setHovered(i)}
            >
              {byteToAscii(b)}
            </span>
          );
        })}
      </div>
    </div>
  );
}, (prev, next) => {
  return (
    prev.showTimestamp === next.showTimestamp &&
    prev.showOffset === next.showOffset &&
    prev.line.offset === next.line.offset &&
    prev.line.timestamp === next.line.timestamp &&
    bytesEqual(prev.line.bytes, next.line.bytes)
  );
});

/// 原始数据显示 — HEX + ASCII 双视图，支持时间戳、发送附加选项、可配置主题
export function RawDataView() {
  const lang = useAppStore((s) => s.lang);
  const clearData = useAppStore((s) => s.clearData);
  const sendText = useAppStore((s) => s.sendText);
  const rawDataVersion = useAppStore((s) => s.rawDataVersion);

  const [view, setView] = useState<'hex' | 'ascii'>('hex');
  const [autoScroll, setAutoScroll] = useState(true);
  const [showTimestamp, setShowTimestamp] = useState(true);
  const [showOffset, setShowOffset] = useState(true);
  const [appendMode, setAppendMode] = useState<AppendMode>('nl');
  const [sendPanelMode, setSendPanelMode] = useState<SendPanelMode>('bottom');
  const [hexColorMode, setHexColorMode] = useState<HexColorMode>('printable');
  const [showSettings, setShowSettings] = useState(false);
  const [snapshot, setSnapshot] = useState<RawDataSnapshot>({ lines: [], totalBytes: 0 });
  const [sendContent, setSendContent] = useState('');
  const contentRef = useRef<HTMLDivElement>(null);
  const userScrolledRef = useRef(false);

  useEffect(() => {
    const unsub = rawDataBuffer.subscribe(setSnapshot);
    setSnapshot(rawDataBuffer.getRecentLines());
    return unsub;
  }, []);

  // 自动滚动：只在 autoScroll 且用户未手动滚动时执行
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
    userScrolledRef.current = !atBottom;
  }, []);

  const handleClear = () => {
    clearData();
    setSnapshot({ lines: [], totalBytes: 0 });
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

  /// 表头：00 01 02 ... 0F
  const headerBytes = useMemo(() => Array.from({ length: BYTES_PER_ROW }, (_, i) => i), []);

  const renderHeader = () => (
    <div className="flex items-center gap-2 px-2 py-1 border-b border-border bg-bg-panel-header sticky top-0 z-10 select-none">
      {showTimestamp && <span className="text-text-secondary text-xs font-mono min-w-[92px] text-right">{t(lang, 'showTimestamp')}</span>}
      {showOffset && <span className="text-text-secondary text-xs font-mono min-w-[72px] text-right">Offset</span>}
      <div className="flex-1 flex gap-0.5">
        {headerBytes.map((b, i) => {
          const isGroupEnd = (i + 1) % GROUP_SIZE === 0 && i !== BYTES_PER_ROW - 1;
          return (
            <span
              key={i}
              className={`inline-flex items-center justify-center text-text-secondary text-xs font-mono w-[22px] h-[18px] ${isGroupEnd ? 'mr-2' : ''}`}
            >
              {byteToHex(b)}
            </span>
          );
        })}
      </div>
      {view === 'hex' && (
        <div className="flex gap-0.5">
          {headerBytes.map((b, i) => {
            const isGroupEnd = (i + 1) % GROUP_SIZE === 0 && i !== BYTES_PER_ROW - 1;
            return (
              <span
                key={i}
                className={`inline-flex items-center justify-center text-text-secondary text-xs font-mono w-[18px] h-[18px] ${isGroupEnd ? 'mr-2' : ''}`}
              >
                {byteToAscii(b)}
              </span>
            );
          })}
        </div>
      )}
    </div>
  );

  const renderContent = () => (
    <div
      className="flex-1 overflow-y-auto overflow-x-hidden font-mono min-h-0"
      ref={contentRef}
      onScroll={handleScroll}
    >
      {renderHeader()}
      <div className="py-1">
        {snapshot.lines.length === 0 ? (
          <div className="flex items-center justify-center h-32 text-text-secondary text-sm">
            {t(lang, 'rawDataEmpty')}
          </div>
        ) : (
          snapshot.lines.map((line) =>
            view === 'hex' ? (
              <HexRow
                key={line.offset}
                line={line}
                showTimestamp={showTimestamp}
                showOffset={showOffset}
                hexColorMode={hexColorMode}
              />
            ) : (
              <AsciiRow
                key={line.offset}
                line={line}
                showTimestamp={showTimestamp}
                showOffset={showOffset}
              />
            )
          )
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
          className={`px-1.5 py-0.5 bg-bg-input border border-border rounded-sm text-text-secondary text-xs font-mono cursor-pointer transition-all hover:border-accent hover:text-text-primary ${appendMode === opt.mode ? 'bg-accent border-accent text-text-bright' : ''}`}
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
      className="px-3 py-1.5 bg-bg-button text-text-bright border-none rounded cursor-pointer text-sm text-center transition-colors hover:bg-bg-button-hover"
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
            className={`px-2 py-0.5 rounded text-xs font-medium transition-colors cursor-pointer ${view === 'hex' ? 'bg-bg-button text-text-bright' : 'text-text-secondary hover:text-text-primary'}`}
            onClick={() => setView('hex')}
          >
            {t(lang, 'hexView')}
          </button>
          <button
            className={`px-2 py-0.5 rounded text-xs font-medium transition-colors cursor-pointer ${view === 'ascii' ? 'bg-bg-button text-text-bright' : 'text-text-secondary hover:text-text-primary'}`}
            onClick={() => setView('ascii')}
          >
            {t(lang, 'asciiView')}
          </button>
        </div>
        <div className="flex-1" />
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
