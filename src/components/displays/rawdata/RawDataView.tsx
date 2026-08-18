import { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import { useAppStore } from '../../../store/appStore';
import { rawDataBuffer, RawDataBuffer } from '../../../lib/buffers/dataBuffer';
import { acquireRawDataNode, releaseRawDataNode } from '../../../lib/buffers/rawDataNodeBuffer';
import { FilteredRawDataBuffer, parseSearchPattern } from '../../../lib/buffers/filteredRawDataBuffer';
import type { RawDataFilterOptions } from '../../../lib/buffers/rawDataSubscription';
import { perfEvent } from '../../../lib/utils/perfLog';
import { useSelection } from '../../../lib/hooks/useSelection';
import { writeTextToClipboard } from '../../../lib/utils/clipboard';
import { rawDataPortId } from '../../../lib/utils/nodeDef';
import '../../../i18n';
import type { RawDataGrouping, RawDataRepr, DirectionFilter, HexColorMode, AppendMode, SendPanelMode } from './rawDataViewHelpers';
import { byteToHex, byteToAscii, formatTime } from './rawDataViewHelpers';
import { DroppedInfoPopover } from '../../common/DroppedInfoPopover';
import { RawDataViewHeader } from './RawDataViewHeader';
import { RawDataViewContent } from './RawDataViewContent';
import { RawDataViewNumericContent } from './RawDataViewNumericContent';
import { RawDataViewSendPanel } from './RawDataViewSendPanel';
import { RawDataViewSettings } from './RawDataViewSettings';
import { getRawDataViewPrefs } from '../../../lib/buffers/rawDataViewStore';

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
  // 目标 Transport (字节源 = 发送目标; null = 自动取第一个)
  // 注意: 选择器必须返回稳定引用 (filter 每次产新数组会触发 useSyncExternalStore 死循环),
  // 故订阅 rfNodes 原始数组, 用 useMemo 派生
  const rfNodes = useAppStore((s) => s.rfNodes);
  const transportNodes = useMemo(
    () => rfNodes.filter((n) => n.type === 'transport' && n.data?.global === true),
    [rfNodes]
  );
  const rawDataSourceNodeId = useAppStore((s) => s.rawDataSourceNodeId);
  const setRawDataSourceNodeId = useAppStore((s) => s.setRawDataSourceNodeId);
  const transportOptions = useMemo(
    () =>
      transportNodes.map((n) => {
        const cfg = (n.data as { config?: { kind?: string } }).config;
        return { id: n.id, label: `${cfg?.kind ?? '?'} (${n.id.slice(-4)})` };
      }),
    [transportNodes]
  );
  const effectiveTransportId =
    rawDataSourceNodeId && transportOptions.some((o) => o.id === rawDataSourceNodeId)
      ? rawDataSourceNodeId
      : (transportOptions[0]?.id ?? null);

  // 持久化 key: widgetId 存在时按控件独立保存, 否则共享 'global' 配置
  const persistKey = widgetId ?? 'global';

  const [grouping, setGrouping] = useState<RawDataGrouping>(() => getRawDataViewPrefs(persistKey).grouping);
  const [repr, setRepr] = useState<RawDataRepr>(() => getRawDataViewPrefs(persistKey).repr);
  const [directionFilter, setDirectionFilter] = useState<DirectionFilter>(() => getRawDataViewPrefs(persistKey).directionFilter);
  const [searchTerm, setSearchTerm] = useState('');
  const [channel, setChannel] = useState<string>('global');
  const [autoScroll, setAutoScroll] = useState(() => getRawDataViewPrefs(persistKey).autoScroll);
  const [showTimestamp, setShowTimestamp] = useState(() => getRawDataViewPrefs(persistKey).showTimestamp);
  const [showOffset, setShowOffset] = useState(() => getRawDataViewPrefs(persistKey).showOffset);
  const [appendMode, setAppendMode] = useState<AppendMode>(() => getRawDataViewPrefs(persistKey).appendMode);
  const [sendPanelMode, setSendPanelMode] = useState<SendPanelMode>(() => getRawDataViewPrefs(persistKey).sendPanelMode);
  const [hexColorMode, setHexColorMode] = useState<HexColorMode>(() => getRawDataViewPrefs(persistKey).hexColorMode);
  const [showSettings, setShowSettings] = useState(false);
  const [droppedInfoOpen, setDroppedInfoOpen] = useState(false);
  const [sendContent, setSendContent] = useState('');
  const [copyFeedback, setCopyFeedback] = useState(false);

  // 通道选择: 该 widget 的入边 (source, sourceHandle) 组合 (去重)
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

  const nodeBufferKey = isDec && selectedChannel ? selectedChannel.sourceId : null;
  const isFiltered = directionFilter !== 'all' || searchTerm.trim() !== '';
  const filterOptions: RawDataFilterOptions = useMemo(
    () => ({ directionFilter, searchTerm: searchTerm.trim() }),
    [directionFilter, searchTerm]
  );

  // 节点 buffer (过滤与否都需要: 过滤包装以它为数据源)
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

  // 过滤模式: 本地增量过滤视图 (复用源 buffer 既有数据, 零额外 IPC)
  const [filteredBuffer, setFilteredBuffer] = useState<FilteredRawDataBuffer | null>(null);
  useEffect(() => {
    if (!isFiltered || isNum) {
      setFilteredBuffer(null);
      return;
    }
    const t0 = performance.now();
    perfEvent(`rawdata filter ON dir=${filterOptions.directionFilter} search="${filterOptions.searchTerm}"`);
    const buf = new FilteredRawDataBuffer(
      nodeBuffer ?? rawDataBuffer,
      filterOptions.directionFilter,
      parseSearchPattern(filterOptions.searchTerm)
    );
    setFilteredBuffer(buf);
    return () => {
      buf.dispose();
      perfEvent(`rawdata filter OFF, 存活 ${(performance.now() - t0).toFixed(0)}ms`);
    };
  }, [isFiltered, isNum, nodeBuffer, filterOptions]);

  // 调试: 长任务监控 — 主线程单次任务 >100ms 即记录 (卡死定位)
  useEffect(() => {
    if (typeof PerformanceObserver === 'undefined') return;
    try {
      const obs = new PerformanceObserver((list) => {
        for (const e of list.getEntries()) {
          console.debug(`[perf] longtask ${e.duration.toFixed(0)}ms`);
        }
      });
      obs.observe({ entryTypes: ['longtask'] });
      return () => obs.disconnect();
    } catch {
      return;
    }
  }, []);

  const buffer = filteredBuffer ?? nodeBuffer ?? rawDataBuffer;

  // 强制重新渲染的版本号
  const [version, setVersion] = useState(0);
  useEffect(() => {
    return buffer.subscribe(() => setVersion((v) => v + 1));
  }, [buffer]);

  // ---- 数值通道视图 ----
  const NUM_MAX_ROWS = 500;
  const [numRows, setNumRows] = useState<Array<{ seq: number; ts: number; value: number }>>([]);
  const numSeqRef = useRef(0);
  const numScrollRef = useRef<HTMLDivElement>(null);

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

  useEffect(() => {
    if (!isNum || !selectedChannel) setNumRows([]);
  }, [isNum, selectedChannel]);

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

  const { clear: clearSelection, ...selection } = useSelection(modeCount);

  useEffect(() => {
    clearSelection();
  }, [clearSelection, grouping, channel]);

  // 自动滚动动画
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
    clearSelection();
    userScrolledRef.current = false;
  };

  const handleSend = () => {
    if (!sendContent || !effectiveTransportId) return;
    let suffix = '';
    switch (appendMode) {
      case 'nl': suffix = '\n'; break;
      case 'tab': suffix = '\t'; break;
      case 'nl_tab': suffix = '\n\t'; break;
      case 'none': suffix = ''; break;
    }
    sendText(effectiveTransportId, sendContent + suffix);
    setSendContent('');
  };

  const copySelected = useCallback(async () => {
    if (selection.selected.size === 0) return;
    const isLine = grouping === 'line';
    const lines = selection.selectedSorted.map((i) => (isLine ? buffer.getNewlineLine(i) : buffer.getLine(i)));
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
      if (e.button !== 0) return;
      selection.handleClick(index, e);
    },
    [selection]
  );

  return (
    <div className="h-full flex flex-col overflow-hidden">
      <RawDataViewHeader
        grouping={grouping}
        repr={repr}
        directionFilter={directionFilter}
        searchTerm={searchTerm}
        channel={channel}
        autoScroll={autoScroll}
        showTimestamp={showTimestamp}
        showSettings={showSettings}
        isNum={isNum}
        isFiltered={isFiltered}
        totalBytes={totalBytes}
        modeCount={modeCount}
        droppedBytes={droppedBytes}
        channelOptions={channelOptions}
        selectionCount={selection.selected.size}
        copyFeedback={copyFeedback}
        userScrolledRef={userScrolledRef}
        lang={lang}
        sourceLabel={sourceLabel}
        onGroupingChange={setGrouping}
        onReprChange={setRepr}
        onDirectionFilterChange={setDirectionFilter}
        onSearchTermChange={setSearchTerm}
        onChannelChange={setChannel}
        onAutoScrollChange={setAutoScroll}
        onShowTimestampChange={setShowTimestamp}
        onShowSettingsChange={setShowSettings}
        onClear={handleClear}
        onClearSelection={clearSelection}
        onCopySelected={() => void copySelected()}
        onDroppedInfoOpen={() => setDroppedInfoOpen(true)}
      />
      <div className="flex-1 flex overflow-hidden min-h-0">
        {sendPanelMode === 'separate' ? (
          <>
            <div className="flex-1 flex flex-col min-w-0 overflow-hidden">
              {isNum ? (
                <div ref={numScrollRef} className="flex-1 flex flex-col min-h-0 overflow-hidden">
                  <RawDataViewNumericContent
                    numRows={numRows}
                    showTimestamp={showTimestamp}
                    lang={lang}
                    grouping={grouping}
                    repr={repr}
                    channel={channel}
                  />
                </div>
              ) : (
                <RawDataViewContent
                  modeCount={modeCount}
                  grouping={grouping}
                  repr={repr}
                  buffer={buffer}
                  showTimestamp={showTimestamp}
                  showOffset={showOffset}
                  hexColorMode={hexColorMode}
                  version={version}
                  lang={lang}
                  selection={selection}
                  onRowMouseDown={handleRowMouseDown}
                  parentRef={parentRef}
                  userScrolledRef={userScrolledRef}
                  isAutoScrollingRef={isAutoScrollingRef}
                  scrollAnimRef={scrollAnimRef}
                  onScroll={handleScroll}
                  onKeyDown={handleKeyDown}
                />
              )}
            </div>
            <div className="w-[220px] flex-shrink-0 border-l border-border bg-bg-sidebar flex flex-col overflow-hidden">
              {showSettings && (
                <div className="flex-1 overflow-y-auto p-3">
                  <RawDataViewSettings
                    hexColorMode={hexColorMode}
                    sendPanelMode={sendPanelMode}
                    showTimestamp={showTimestamp}
                    showOffset={showOffset}
                    onHexColorModeChange={setHexColorMode}
                    onSendPanelModeChange={setSendPanelMode}
                    onShowTimestampChange={setShowTimestamp}
                    onShowOffsetChange={setShowOffset}
                    lang={lang}
                  />
                </div>
              )}
              <div className="border-t border-border p-2 flex flex-col gap-1.5">
                <RawDataViewSendPanel
                  appendMode={appendMode}
                  sendContent={sendContent}
                  onAppendModeChange={setAppendMode}
                  onSendContentChange={setSendContent}
                  onSend={handleSend}
                  lang={lang}
                  compact
                  transports={transportOptions}
                  selectedTransport={effectiveTransportId}
                  onTransportChange={setRawDataSourceNodeId}
                />
              </div>
            </div>
          </>
        ) : (
          <div className="flex-1 flex flex-col min-w-0 overflow-hidden">
            {isNum ? (
              <div ref={numScrollRef} className="flex-1 flex flex-col min-h-0 overflow-hidden">
                <RawDataViewNumericContent
                  numRows={numRows}
                  showTimestamp={showTimestamp}
                  lang={lang}
                  grouping={grouping}
                  repr={repr}
                  channel={channel}
                />
              </div>
            ) : (
              <RawDataViewContent
                modeCount={modeCount}
                grouping={grouping}
                repr={repr}
                buffer={buffer}
                showTimestamp={showTimestamp}
                showOffset={showOffset}
                hexColorMode={hexColorMode}
                version={version}
                lang={lang}
                selection={selection}
                onRowMouseDown={handleRowMouseDown}
                parentRef={parentRef}
                userScrolledRef={userScrolledRef}
                isAutoScrollingRef={isAutoScrollingRef}
                scrollAnimRef={scrollAnimRef}
                onScroll={handleScroll}
                onKeyDown={handleKeyDown}
              />
            )}
            {showSettings && (
              <div className="border-t border-border p-3 bg-bg-sidebar overflow-y-auto max-h-[180px]">
                <RawDataViewSettings
                  hexColorMode={hexColorMode}
                  sendPanelMode={sendPanelMode}
                  showTimestamp={showTimestamp}
                  showOffset={showOffset}
                  onHexColorModeChange={setHexColorMode}
                  onSendPanelModeChange={setSendPanelMode}
                  onShowTimestampChange={setShowTimestamp}
                  onShowOffsetChange={setShowOffset}
                  lang={lang}
                />
              </div>
            )}
            <RawDataViewSendPanel
              appendMode={appendMode}
              sendContent={sendContent}
              onAppendModeChange={setAppendMode}
              onSendContentChange={setSendContent}
              onSend={handleSend}
              lang={lang}
              transports={transportOptions}
              selectedTransport={effectiveTransportId}
              onTransportChange={setRawDataSourceNodeId}
            />
          </div>
        )}
      </div>
      <DroppedInfoPopover
        open={droppedInfoOpen}
        onClose={() => setDroppedInfoOpen(false)}
        variant="rawdata"
      />
    </div>
  );
}
