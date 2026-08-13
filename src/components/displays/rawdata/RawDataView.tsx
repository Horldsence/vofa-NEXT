import { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { useAppStore } from '../../../store/appStore';
import { rawDataBuffer, type RawDataBuffer } from '../../../lib/buffers/dataBuffer';
import { acquireRawDataNode, releaseRawDataNode } from '../../../lib/buffers/rawDataNodeBuffer';
import { useSelection } from '../../../lib/hooks/useSelection';
import { writeTextToClipboard } from '../../../lib/utils/clipboard';
import { rawDataPortId } from '../../../lib/utils/nodeDef';
import { t } from '../../../i18n';
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
import { AppendMode, SendPanelMode, HexColorMode, ROW_HEIGHT, HeaderBytes, byteToHex, byteToAscii, formatTime, type RawDataGrouping, type RawDataRepr } from './rawDataViewHelpers';
import { Row } from './RawDataRow';
import { useRawDataViewStore, getRawDataViewPrefs } from '../../../lib/buffers/rawDataViewStore';

const GROUPING_OPTIONS: { value: RawDataGrouping; label: string }[] = [
  { value: 'grid', label: 'gridView' },
  { value: 'line', label: 'lineView' },
];

const REPR_OPTIONS: { value: RawDataRepr; label: string }[] = [
  { value: 'hex', label: 'hexView' },
  { value: 'ascii', label: 'asciiView' },
];

/// 原始数据显示 — Grid/Line × HEX/ASCII 四视图, 支持虚拟滚动、文本选中/行选中复制、时间戳、发送
/// widgetId 存在时展示通道选择器: FrameDecoder 的 raw 口 = 该节点独立整帧字节流,
/// field 口及其他数值源 = 数值流 (graphOutputs)
export function RawDataView({ widgetId }: { widgetId?: string }) {
  const lang = useAppStore((s) => s.lang);
  const clearData = useAppStore((s) => s.clearData);
  const sendText = useAppStore((s) => s.sendText);
  const rfEdges = useAppStore((s) => s.rfEdges);
  const widgets = useAppStore((s) => s.widgets);
  const graphOutputs = useAppStore((s) => s.graphOutputs);
  const graphOutputsTick = useAppStore((s) => s.graphOutputsTick);

  // 持久化 key: widgetId 存在时按控件独立保存, 否则共享 'global' 配置
  const persistKey = widgetId ?? 'global';

  const [grouping, setGrouping] = useState<RawDataGrouping>(() => getRawDataViewPrefs(persistKey).grouping);
  const [repr, setRepr] = useState<RawDataRepr>(() => getRawDataViewPrefs(persistKey).repr);
  const [channel, setChannel] = useState<string>('global');
  const [autoScroll, setAutoScroll] = useState(() => getRawDataViewPrefs(persistKey).autoScroll);
  const [showTimestamp, setShowTimestamp] = useState(() => getRawDataViewPrefs(persistKey).showTimestamp);
  const [showOffset, setShowOffset] = useState(() => getRawDataViewPrefs(persistKey).showOffset);
  const [appendMode, setAppendMode] = useState<AppendMode>(() => getRawDataViewPrefs(persistKey).appendMode);
  const [sendPanelMode, setSendPanelMode] = useState<SendPanelMode>(() => getRawDataViewPrefs(persistKey).sendPanelMode);
  const [hexColorMode, setHexColorMode] = useState<HexColorMode>(() => getRawDataViewPrefs(persistKey).hexColorMode);
  const [showSettings, setShowSettings] = useState(false);
  const [sendContent, setSendContent] = useState('');
  const [copyFeedback, setCopyFeedback] = useState(false);

  // 配置变更时按 persistKey 写入 store (不经订阅, 避免自身写入触发重渲染/无限循环)
  useEffect(() => {
    useRawDataViewStore.getState().setPrefs(persistKey, {
      grouping,
      repr,
      showTimestamp,
      showOffset,
      autoScroll,
      hexColorMode,
      sendPanelMode,
      appendMode,
    });
  }, [persistKey, grouping, repr, showTimestamp, showOffset, autoScroll, hexColorMode, sendPanelMode, appendMode]);

  // 通道选择: 该 widget 的入边 (source, sourceHandle) 组合 (去重)
  // 通道 key = `src:<sourceId>:<sourceHandle>` (与 WidgetNode 动态端口 id 一致), 即 select option 的 value
  const channelOptions = useMemo(() => {
    if (!widgetId) return [];
    const seen = new Set<string>();
    const options: { key: string; sourceId: string; sourceHandle: string | undefined }[] = [];
    for (const e of rfEdges) {
      if (e.target !== widgetId) continue;
      const sourceHandle = e.sourceHandle ?? undefined;
      const key = rawDataPortId(e.source, sourceHandle);
      if (seen.has(key)) continue;
      seen.add(key);
      options.push({ key, sourceId: e.source, sourceHandle });
    }
    return options;
  }, [widgetId, rfEdges]);

  const sourceLabel = useCallback(
    (id: string) => {
      const w = widgets.find((w) => w.params.id === id);
      return w && 'label' in w.params ? w.params.label : id;
    },
    [widgets]
  );

  const sourceIsFrameDecoder = useCallback(
    (id: string) => widgets.some((w) => w.kind === 'FrameDecoder' && w.params.id === id),
    [widgets]
  );

  // 当前选中通道 → 分类:
  // - FrameDecoder 的 raw 口 = 该解码器消费的整帧原始字节 (节点独立字节流)
  // - 其余 (FrameDecoder field 口 / ChannelSource / Math / Filter / ...) = 数值流 (graphOutputs)
  const selectedChannel = channelOptions.find((o) => o.key === channel);
  const isDec =
    !!selectedChannel &&
    selectedChannel.sourceHandle === 'raw' &&
    sourceIsFrameDecoder(selectedChannel.sourceId);
  const isNum = !!selectedChannel && !isDec;

  // 切换控件 / 通道消失时回退到 global
  useEffect(() => setChannel('global'), [widgetId]);
  useEffect(() => {
    if (channel === 'global') return;
    if (channelOptions.length === 0 || !channelOptions.some((o) => o.key === channel)) setChannel('global');
  }, [channelOptions, channel]);

  // 选中 FrameDecoder 通道时, 通过注册表获取该节点的独立 buffer (该解码器消费的原始帧字节)
  const nodeBufferKey = isDec && selectedChannel ? selectedChannel.sourceId : null;
  const [nodeBuffer, setNodeBuffer] = useState<RawDataBuffer | null>(null);

  useEffect(() => {
    if (!nodeBufferKey) {
      setNodeBuffer(null);
      return;
    }
    const acquired = acquireRawDataNode(nodeBufferKey);
    setNodeBuffer(acquired);
    return () => releaseRawDataNode(nodeBufferKey);
  }, [nodeBufferKey]);

  // 数据源统一抽象: 节点模式读节点 buffer, 否则读全局 buffer
  const buffer = nodeBuffer ?? rawDataBuffer;

  // 强制重新渲染的版本号 (RAF 节流后递增)
  const [version, setVersion] = useState(0);
  useEffect(() => {
    return buffer.subscribe(() => setVersion((v) => v + 1));
  }, [buffer]);

  // ---- 数值通道视图 ----
  // ChannelSource ch0..chN / Math / Filter 等的输出从 store.graphOutputs 读取 (后端 60 FPS 推送)
  const NUM_MAX_ROWS = 500;
  const [numRows, setNumRows] = useState<Array<{ seq: number; ts: number; value: number }>>([]);
  const numSeqRef = useRef(0);
  const numScrollRef = useRef<HTMLDivElement>(null);

  // graphOutputs 由后端 ticker 每 16ms 无条件推送 (断开后引用仍每帧变化), 不能作为 effect 依赖;
  // 用 ref 采样最新值, 依赖只保留 graphOutputsTick (仅真实帧评估时递增, 断开后不变)。
  const graphOutputsRef = useRef(graphOutputs);
  graphOutputsRef.current = graphOutputs;

  useEffect(() => {
    if (!isNum || !selectedChannel) return;
    const handle = selectedChannel.sourceHandle ?? 'data';
    const v = graphOutputsRef.current[selectedChannel.sourceId]?.[handle];
    if (v === undefined) return;
    setNumRows((prev) => {
      const next = [...prev, { seq: numSeqRef.current++, ts: Date.now(), value: v }];
      return next.length > NUM_MAX_ROWS ? next.slice(-NUM_MAX_ROWS) : next;
    });
  }, [graphOutputsTick, isNum, selectedChannel]);

  // 离开数值通道或切换通道时清空历史
  useEffect(() => {
    if (!isNum || !selectedChannel) setNumRows([]);
  }, [isNum, selectedChannel]);

  // 数值视图自动滚动到底部
  useEffect(() => {
    if (!autoScroll) return;
    const el = numScrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [numRows, autoScroll]);

  const lineCount = buffer.lineCount;
  const modeCount = grouping === 'line' ? buffer.newlineLineCount : lineCount;
  const totalBytes = buffer.totalBytes;
  const droppedBytes = buffer.droppedBytes;

  const parentRef = useRef<HTMLDivElement>(null);
  const userScrolledRef = useRef(false);
  const isAutoScrollingRef = useRef(false);
  const scrollAnimRef = useRef<number | null>(null);

  const virtualizer = useVirtualizer({
    count: modeCount,
    getScrollElement: () => parentRef.current,
    // 固定行高: estimateSize 返回常量, 跳过 measureElement 的 DOM 测量开销,
    // 行内容按 buffer 实时读取, 无行高变化, 保证 60fps 滚动
    estimateSize: () => ROW_HEIGHT,
    overscan: 5,
    // 行无唯一 id, 追加型缓冲区中 index 即稳定身份 (视图切换时按 index 重取行)
    getItemKey: (index) => index,
  });

  const selection = useSelection(modeCount);

  // 自动滚动 — rAF 缓动动画 (~250ms easeOutCubic), 新数据到达时平滑跟随到底部
  // isAutoScrollingRef 在整个动画期间保持 true, 动画跑完才复位:
  // 若在动画中途复位, 滚动事件会被 handleScroll 误判为"用户滚动" (atBottom 为 false),
  // 从而永久禁用自动滚动。
  useEffect(() => {
    if (!autoScroll) {
      isAutoScrollingRef.current = false;
      return;
    }
    if (userScrolledRef.current || modeCount === 0) return;
    isAutoScrollingRef.current = true;
    const el = parentRef.current;
    if (el) {
      const start = el.scrollTop;
      const duration = 250;
      const t0 = performance.now();
      const easeOutCubic = (p: number) => 1 - Math.pow(1 - p, 3);
      const step = (now: number) => {
        const p = Math.min(1, (now - t0) / duration);
        // 每帧取最新底部, 让动画目标跟随持续到达的新数据
        const target = Math.max(0, el.scrollHeight - el.clientHeight);
        el.scrollTop = start + (target - start) * easeOutCubic(p);
        if (p < 1) {
          scrollAnimRef.current = requestAnimationFrame(step);
        } else {
          scrollAnimRef.current = null;
          isAutoScrollingRef.current = false;
        }
      };
      scrollAnimRef.current = requestAnimationFrame(step);
    }
    return () => {
      if (scrollAnimRef.current !== null) {
        cancelAnimationFrame(scrollAnimRef.current);
        scrollAnimRef.current = null;
      }
    };
  }, [modeCount, autoScroll, version, buffer]);

  // 检测用户手动滚动
  const handleScroll = useCallback(() => {
    if (isAutoScrollingRef.current || !parentRef.current) return;
    const el = parentRef.current;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 30;
    userScrolledRef.current = !atBottom;
  }, []);

  const handleClear = () => {
    if (isNum) {
      setNumRows([]);
      return;
    }
    clearData();
    if (buffer !== rawDataBuffer) buffer.clear();
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
    const isLine = grouping === 'line';
    const lines = indices.map((i) => (isLine ? buffer.getNewlineLine(i) : buffer.getLine(i)));
    const text = lines
      .map((line) => {
        const hex = Array.from(line.bytes, (b) => byteToHex(b)).join(' ');
        const ascii = Array.from(line.bytes, (b) => byteToAscii(b)).join('');
        if (repr === 'ascii') {
          return `${formatTime(line.timestamp)}  ${ascii}`;
        }
        if (isLine) {
          return `${formatTime(line.timestamp)}  ${hex}  |${ascii}|`;
        }
        return `${formatTime(line.timestamp)} ${line.offset.toString(16).padStart(8, '0').toUpperCase()}  ${hex.padEnd(48, ' ')}  |${ascii}|`;
      })
      .join('\n');
    const ok = await writeTextToClipboard(text);
    if (ok) {
      setCopyFeedback(true);
      setTimeout(() => setCopyFeedback(false), 1200);
    }
  }, [selection.selectedSorted, grouping, repr, buffer]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      const target = e.target as HTMLElement;
      if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable) return;
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'a') {
        e.preventDefault();
        selection.selectAll();
      }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'c') {
        // 有原生文本选区且没有行选择时, 让浏览器执行原生复制
        const native = window.getSelection();
        if (selection.selected.size === 0 && native && !native.isCollapsed) return;
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
      {showOffset && grouping === 'grid' && (
        <span className="text-text-secondary text-xs font-mono min-w-[80px] text-right">Offset</span>
      )}
      {grouping === 'line' ? (
        <>
          <div className="flex-1" />
          {repr === 'hex' && (
            <div className="flex gap-0.5">
              <span className="text-text-secondary text-xs font-mono">{t(lang, 'asciiView')}</span>
            </div>
          )}
        </>
      ) : repr === 'hex' ? (
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

  const renderNumericContent = () => (
    <div
      key={`${grouping}:${repr}:${channel}`}
      className="flex-1 flex flex-col min-h-0 overflow-hidden font-mono animate-rawdata-enter select-text"
    >
      <div className="flex-1 overflow-auto min-h-0" ref={numScrollRef}>
        {numRows.length === 0 ? (
          <div className="flex items-center justify-center h-32 text-text-secondary text-sm">
            {t(lang, 'rawDataEmpty')}
          </div>
        ) : (
          numRows.map((r) => (
            <div key={r.seq} className="flex items-center gap-2 px-2 text-xs font-mono animate-rawdata-row">
              {showTimestamp && (
                <span className="text-accent min-w-[92px] text-right">{formatTime(r.ts)}</span>
              )}
              <span className="text-text-primary">
                {Number.isInteger(r.value) ? r.value.toFixed(0) : r.value.toFixed(4)}
              </span>
            </div>
          ))
        )}
      </div>
    </div>
  );

  const renderContent = () => (
    <div
      key={`${grouping}:${repr}:${channel}`}
      className="flex-1 flex flex-col min-h-0 overflow-hidden font-mono animate-rawdata-enter"
    >
      {renderHeader()}
      <div
        className="flex-1 overflow-auto min-h-0 outline-none"
        ref={parentRef}
        onScroll={handleScroll}
        onKeyDown={handleKeyDown}
        tabIndex={0}
      >
        {modeCount === 0 ? (
          <div className="flex items-center justify-center h-32 text-text-secondary text-sm">
            {t(lang, 'rawDataEmpty')}
          </div>
        ) : (
          <div style={{ height: `${virtualizer.getTotalSize()}px`, position: 'relative' }}>
            <div
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: '100%',
                minWidth: grouping === 'line' ? 'max-content' : undefined,
                transform: `translateY(${virtualItems[0]?.start ?? 0}px)`,
              }}
            >
              {virtualItems.map((virtualRow) => (
                <Row
                  key={virtualRow.key}
                  index={virtualRow.index}
                  grouping={grouping}
                  repr={repr}
                  buffer={buffer}
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
          {GROUPING_OPTIONS.map((opt) => (
            <button
              key={opt.value}
              disabled={isNum}
              className={`px-2 py-0.5 rounded text-xs font-medium transition-all duration-150 motion-safe:active:scale-95 cursor-pointer disabled:cursor-not-allowed disabled:pointer-events-none disabled:opacity-40 ${grouping === opt.value ? 'bg-bg-button text-text-inverse' : 'text-text-secondary hover:text-text-primary'}`}
              onClick={() => setGrouping(opt.value)}
            >
              {t(lang, opt.label)}
            </button>
          ))}
        </div>
        <div className="flex items-center bg-bg-input rounded p-0.5">
          {REPR_OPTIONS.map((opt) => (
            <button
              key={opt.value}
              disabled={isNum}
              className={`px-2 py-0.5 rounded text-xs font-medium transition-all duration-150 motion-safe:active:scale-95 cursor-pointer disabled:cursor-not-allowed disabled:pointer-events-none disabled:opacity-40 ${repr === opt.value ? 'bg-bg-button text-text-inverse' : 'text-text-secondary hover:text-text-primary'}`}
              onClick={() => setRepr(opt.value)}
            >
              {t(lang, opt.label)}
            </button>
          ))}
        </div>

        {channelOptions.length > 0 && (
          <label className="flex items-center gap-1 text-xs text-text-secondary flex-shrink-0">
            <span>{t(lang, 'rawDataChannel')}</span>
            <select
              value={channel}
              onChange={(e) => setChannel(e.target.value)}
              className="bg-bg-input border border-border rounded px-1 py-0.5 text-xs font-mono text-text-primary transition-colors hover:border-accent focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent/40 cursor-pointer max-w-[160px]"
            >
              <option value="global">{t(lang, 'rawDataGlobal')}</option>
              {channelOptions.map((o) => (
                <option key={o.key} value={o.key}>
                  {o.sourceHandle || sourceLabel(o.sourceId)}
                </option>
              ))}
            </select>
          </label>
        )}

        <div className={`flex items-center gap-1 text-text-secondary text-xs font-mono ${isNum ? 'opacity-40' : ''}`}>
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
              disabled={isNum}
              className={`w-7 h-7 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-all duration-150 motion-safe:active:scale-95 cursor-pointer disabled:cursor-not-allowed disabled:pointer-events-none disabled:opacity-40 ${copyFeedback ? 'text-green' : ''}`}
              title={t(lang, 'copySelected')}
              onClick={() => void copySelected()}
            >
              {copyFeedback ? <Check size={14} /> : <Copy size={14} />}
            </button>
            <button
              disabled={isNum}
              className="w-7 h-7 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-all duration-150 motion-safe:active:scale-95 cursor-pointer disabled:cursor-not-allowed disabled:pointer-events-none disabled:opacity-40"
              title={t(lang, 'clearSelection')}
              onClick={selection.clear}
            >
              <X size={14} />
            </button>
          </>
        )}

        <button
          className={`w-7 h-7 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-all duration-150 motion-safe:active:scale-95 cursor-pointer ${showTimestamp ? 'text-text-bright bg-bg-hover' : ''}`}
          title={t(lang, 'showTimestamp')}
          onClick={() => setShowTimestamp(!showTimestamp)}
        >
          <Clock size={14} />
        </button>
        <button
          className={`w-7 h-7 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-all duration-150 motion-safe:active:scale-95 cursor-pointer ${autoScroll && !userScrolledRef.current ? 'text-text-bright bg-bg-hover' : ''}`}
          title={t(lang, 'autoScroll')}
          onClick={() => {
            setAutoScroll(!autoScroll);
            userScrolledRef.current = false;
          }}
        >
          <ArrowDown size={14} />
        </button>
        <button
          className={`w-7 h-7 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-all duration-150 motion-safe:active:scale-95 cursor-pointer ${showSettings ? 'text-text-bright bg-bg-hover' : ''}`}
          title={t(lang, 'settings')}
          onClick={() => setShowSettings(!showSettings)}
        >
          <Settings2 size={14} />
        </button>
        <button
          className="w-7 h-7 flex items-center justify-center rounded text-text-secondary hover:bg-bg-danger hover:text-text-bright transition-all duration-150 motion-safe:active:scale-95 cursor-pointer"
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
              {isNum ? renderNumericContent() : renderContent()}
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
            {isNum ? renderNumericContent() : renderContent()}
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
