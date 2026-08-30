import { useState, useEffect, useRef, useCallback, useMemo, useSyncExternalStore } from 'react';
import { Unplug } from 'lucide-react';
import { useAppStore } from '../../../store/appStore';
import { RawDataBuffer } from '../../../lib/buffers/dataBuffer';
import { acquireRawDataNode, releaseRawDataNode } from '../../../lib/buffers/rawDataNodeBuffer';
import { acquireRawDataTransport, releaseRawDataTransport } from '../../../lib/buffers/rawDataTransportBuffer';
import { classifyRawDataChannel, resolveRawDataChannelKey } from '../../../lib/utils/rawDataChannel';
import type { RawDataFilterOptions } from '../../../lib/buffers/rawDataSubscription';
import { useSelection } from '../../../lib/hooks/useSelection';
import { writeTextToClipboard } from '../../../lib/utils/clipboard';
import { rawDataPortId } from '../../../lib/utils/nodeDef';
import { traceTransportSource } from '../../../store/appStoreHelpers';
import { t } from '../../../i18n';
import type { RawDataGrouping, RawDataRepr, DirectionFilter, HexColorMode, AppendMode, SendPanelMode } from './rawDataViewHelpers';
import { byteToHex, byteToAscii, formatTime } from './rawDataViewHelpers';
import { DroppedInfoPopover } from '../../common/DroppedInfoPopover';
import { RawDataViewHeader } from './RawDataViewHeader';
import { RawDataViewContent } from './RawDataViewContent';
import { RawDataViewNumericContent } from './RawDataViewNumericContent';
import { RawDataViewSendPanel } from './RawDataViewSendPanel';
import { RawDataViewSettings } from './RawDataViewSettings';
import { getRawDataViewPrefs } from '../../../lib/buffers/rawDataViewStore';
import { getPortSampleStore } from '../../../lib/data/dataClient';

/// 原始数据显示 — Grid/Line × HEX/ASCII 四视图, 支持虚拟滚动、文本选中/行选中复制、时间戳、发送
/// 纯端口制: 每张卡片独立的输入选择 (存 RawDataConfig.selectedInput), 选择器只列该卡片
/// 已连接的端口 (Transport.rx / Protocol.out / FrameDecoder.raw / 数值口); 无连线时空态引导。
/// FrameDecoder 的 raw 口 = 该节点独立整帧字节流, field 口及其他数值源 = 数值流 (graphOutputs)
export function RawDataView({ widgetId }: { widgetId?: string }) {
  const lang = useAppStore((s) => s.lang);
  const clearData = useAppStore((s) => s.clearData);
  const sendText = useAppStore((s) => s.sendText);
  const rfEdges = useAppStore((s) => s.rfEdges);
  const widgets = useAppStore((s) => s.widgets);
  const updateWidget = useAppStore((s) => s.updateWidget);
  // 注意: 选择器必须返回稳定引用 (filter 每次产新数组会触发 useSyncExternalStore 死循环),
  // 故订阅 rfNodes 原始数组, 用 useMemo 派生
  const rfNodes = useAppStore((s) => s.rfNodes);
  // 持久化 key: widgetId 存在时按控件独立保存, 否则共享 'global' 配置
  const persistKey = widgetId ?? 'global';

  const [grouping, setGrouping] = useState<RawDataGrouping>(() => getRawDataViewPrefs(persistKey).grouping);
  const [repr, setRepr] = useState<RawDataRepr>(() => getRawDataViewPrefs(persistKey).repr);
  const [directionFilter, setDirectionFilter] = useState<DirectionFilter>(() => getRawDataViewPrefs(persistKey).directionFilter);
  const [searchTerm, setSearchTerm] = useState('');
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
  // 字节平面源 (Transport/Protocol 全局节点) 附带节点标签, 避免多接口时选项同为 "rx"/"out" 无法区分
  const channelOptions = useMemo(() => {
    if (!widgetId) return [];
    const globalLabel = (id: string): string | null => {
      const n = rfNodes.find((n) => n.id === id);
      const cfg = (n?.data as { config?: { kind?: string } } | undefined)?.config;
      if (n?.type === 'transport') return `${cfg?.kind ?? '?'} (${id.slice(-4)})`;
      if (n?.type === 'protocol') return cfg?.kind ?? 'Protocol';
      return null;
    };
    const seen = new Set<string>();
    const options: { key: string; sourceId: string; sourceHandle: string | undefined; label?: string }[] = [];
    for (const e of rfEdges) {
      if (e.target !== widgetId) continue;
      const sourceHandle = e.sourceHandle ?? undefined;
      const key = rawDataPortId(e.source, sourceHandle);
      if (seen.has(key)) continue;
      seen.add(key);
      const g = globalLabel(e.source);
      options.push({
        key,
        sourceId: e.source,
        sourceHandle,
        label: g ? `${g}·${sourceHandle ?? 'data'}` : undefined,
      });
    }
    return options;
  }, [widgetId, rfEdges, rfNodes]);

  // 纯端口制通道选择 (单一事实源 = 控件配置 RawDataConfig.selectedInput):
  // 配置选中且该连线仍存在 → 用它; 否则回退第一个已连接端口; 无连线 → '' (空态)。
  // 切换选择经 onChannelChange 写回配置, 触发操作历史与图同步 (Sink 节点无参数变化, 重编译无害)
  const ownWidget = useMemo(() => {
    const w = widgets.find((w) => w.kind === 'RawData' && w.params.id === widgetId);
    return w?.kind === 'RawData' ? w : undefined;
  }, [widgets, widgetId]);
  const channel = useMemo(
    () => resolveRawDataChannelKey(ownWidget?.params.selectedInput, channelOptions) ?? '',
    [channelOptions, ownWidget]
  );

  const onChannelChange = useCallback(
    (key: string) => {
      if (!ownWidget) return;
      updateWidget(ownWidget.params.id, {
        kind: 'RawData',
        params: { ...ownWidget.params, selectedInput: key },
      });
    },
    [ownWidget, updateWidget]
  );

  const sourceLabel = useCallback(
    (id: string) => {
      const w = widgets.find((w) => w.params.id === id);
      return w && 'label' in w.params ? w.params.label : id;
    },
    [widgets]
  );

  const selectedChannel = channelOptions.find((o) => o.key === channel);
  // 通道分类: FrameDecoder raw 口 = 节点旁路字节流; Transport/Protocol 源 = 接口原始字节流;
  // 其余 = 数值流 (graphOutputs)
  const channelInfo = useMemo(
    () =>
      selectedChannel
        ? classifyRawDataChannel(selectedChannel, rfNodes, rfEdges, widgets)
        : null,
    [selectedChannel, rfNodes, rfEdges, widgets]
  );
  const isDec = channelInfo?.kind === 'decoder-node';
  const isByteSrc = channelInfo?.kind === 'byte-source';
  const isNum = !!selectedChannel && !isDec && !isByteSrc;

  // 发送目标 = 选中通道沿连线溯源到的 Transport (字节源/数值口均可上溯);
  // 溯源失败 (如自定义控件数值口) → null, 发送面板禁用
  const sendTargetId = useMemo(() => {
    if (!selectedChannel) return null;
    return traceTransportSource(selectedChannel.sourceId, rfEdges, rfNodes);
  }, [selectedChannel, rfEdges, rfNodes]);

  // 面板状态徽章的可观察 Transport: 字节源 = 通道字节源; 数值口 = 发送上溯目标;
  // FrameDecoder raw 口 = null (节点旁路, 无固定连接语义)
  const viewTransportId = isByteSrc
    ? (channelInfo?.transportId ?? null)
    : isNum
      ? sendTargetId
      : null;
  const viewConnState = useAppStore((s) =>
    viewTransportId ? (s.connectionStates[viewTransportId] ?? 'Disconnected') : null
  );
  const sendTargetLabel = sendTargetId
    ? (() => {
        const n = rfNodes.find((n) => n.id === sendTargetId);
        const cfg = (n?.data as { config?: { kind?: string } } | undefined)?.config;
        return n ? `${cfg?.kind ?? '?'} (${sendTargetId.slice(-4)})` : null;
      })()
    : null;

  const nodeBufferKey = isDec && selectedChannel ? selectedChannel.sourceId : null;
  const isFiltered = directionFilter !== 'all' || searchTerm.trim() !== '';
  const filterOptions: RawDataFilterOptions = useMemo(
    () => ({ directionFilter, searchTerm: searchTerm.trim() }),
    [directionFilter, searchTerm]
  );

  const backendFilter = isFiltered ? filterOptions : undefined;

  // 节点 buffer。方向和搜索由后端订阅源执行。
  const [nodeBuffer, setNodeBuffer] = useState<RawDataBuffer | null>(null);
  useEffect(() => {
    if (!nodeBufferKey) {
      setNodeBuffer(null);
      return;
    }
    const acquired = acquireRawDataNode(nodeBufferKey, backendFilter);
    setNodeBuffer(acquired);
    return () => releaseRawDataNode(nodeBufferKey, backendFilter);
  }, [nodeBufferKey, backendFilter]);

  // 字节源通道 buffer: 按 Transport 引用计数获取 (同 Transport 多卡片自动共享同一订阅);
  // 上溯失败 (无 transportId) 用空 buffer 占位；RawData 不再维持隐藏的全局订阅。
  const byteTransportId = isByteSrc ? (channelInfo?.transportId ?? null) : null;
  const transportBufferKey = byteTransportId ?? null;
  const [transportBuffer, setTransportBuffer] = useState<RawDataBuffer | null>(null);
  useEffect(() => {
    if (!transportBufferKey) {
      setTransportBuffer(null);
      return;
    }
    const acquired = acquireRawDataTransport(transportBufferKey, backendFilter);
    setTransportBuffer(acquired);
    return () => releaseRawDataTransport(transportBufferKey, backendFilter);
  }, [transportBufferKey, backendFilter]);

  const emptyByteBufferRef = useRef<RawDataBuffer | null>(null);
  // 惰性取空 buffer (占位: 无 transportId / 无连线时保持订阅链类型完整)
  const getEmptyByteBuffer = useCallback((): RawDataBuffer => {
    if (!emptyByteBufferRef.current) emptyByteBufferRef.current = new RawDataBuffer();
    return emptyByteBufferRef.current;
  }, []);
  const byteSourceBuffer = !isByteSrc
    ? null
    : byteTransportId
      ? transportBuffer
      : getEmptyByteBuffer();

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

  const buffer = nodeBuffer ?? byteSourceBuffer ?? getEmptyByteBuffer();

  // 强制重新渲染的版本号
  const [version, setVersion] = useState(0);
  useEffect(() => {
    return buffer.subscribe(() => setVersion((v) => v + 1));
  }, [buffer]);

  // ---- 数值通道视图 ----
  const numScrollRef = useRef<HTMLDivElement>(null);
  const sampleStore = useMemo(
    () =>
      getPortSampleStore(
        isNum ? selectedChannel?.sourceId : undefined,
        isNum ? (selectedChannel?.sourceHandle ?? 'data') : undefined
      ),
    [isNum, selectedChannel?.sourceId, selectedChannel?.sourceHandle]
  );
  const sampleSnapshot = useSyncExternalStore(
    sampleStore.subscribe,
    sampleStore.getSnapshot,
    sampleStore.getSnapshot
  );
  const numRows = sampleSnapshot.rows;

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
    userScrolledRef.current = false;
  }, [clearSelection, grouping, channel]);

  // 自动滚动到最新结果：每次数据帧只安排一次末尾锚定，不做会持续追赶的平滑动画。
  useEffect(() => {
    if (!autoScroll) {
      isAutoScrollingRef.current = false;
      return;
    }
    const activeCount = isNum ? numRows.length : modeCount;
    if (userScrolledRef.current || activeCount === 0) return;
    if (scrollAnimRef.current !== null) cancelAnimationFrame(scrollAnimRef.current);
    scrollAnimRef.current = requestAnimationFrame(() => {
      scrollAnimRef.current = null;
      const el = isNum ? numScrollRef.current : parentRef.current;
      if (!el) return;
      isAutoScrollingRef.current = true;
      el.scrollTop = el.scrollHeight;
      requestAnimationFrame(() => {
        isAutoScrollingRef.current = false;
      });
    });
    return () => {
      if (scrollAnimRef.current !== null) {
        cancelAnimationFrame(scrollAnimRef.current);
        scrollAnimRef.current = null;
      }
    };
  }, [modeCount, numRows.length, sampleSnapshot.version, isNum, autoScroll, version, buffer]);

  const handleScroll = useCallback(() => {
    if (isAutoScrollingRef.current) return;
    const el = isNum ? numScrollRef.current : parentRef.current;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 30;
    userScrolledRef.current = !atBottom;
  }, [isNum]);

  const handleClear = () => {
    if (isNum) {
      sampleStore.clear();
      return;
    }
    clearData();
    buffer.clear();
    clearSelection();
    userScrolledRef.current = false;
  };

  const handleSend = () => {
    if (!sendContent || !sendTargetId) return;
    let suffix = '';
    switch (appendMode) {
      case 'nl': suffix = '\n'; break;
      case 'tab': suffix = '\t'; break;
      case 'nl_tab': suffix = '\n\t'; break;
      case 'none': suffix = ''; break;
    }
    sendText(sendTargetId, sendContent + suffix);
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

  // 无连线空态: 纯端口制下没有可显示的输入 → 引导用户先建立连线, 不订阅任何数据
  if (!selectedChannel) {
    return (
      <div className="h-full flex flex-col items-center justify-center gap-2 text-text-secondary">
        <Unplug size={22} className="opacity-50" />
        <span className="text-xs">{t(lang, 'rawDataNoInputTitle')}</span>
        <span className="text-[10px] opacity-70 px-6 text-center break-all">
          {t(lang, 'rawDataNoInputHint')}
        </span>
      </div>
    );
  }

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
        connState={viewConnState}
        onGroupingChange={setGrouping}
        onReprChange={setRepr}
        onDirectionFilterChange={setDirectionFilter}
        onSearchTermChange={setSearchTerm}
        onChannelChange={onChannelChange}
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
                <div className="flex-1 flex flex-col min-h-0 overflow-hidden">
                  <RawDataViewNumericContent
                    numRows={numRows}
                    status={sampleSnapshot.status}
                    previewSkipped={sampleSnapshot.previewSkipped}
                    retentionEvicted={sampleSnapshot.retentionEvicted}
                    ingressDropped={sampleSnapshot.ingressDropped}
                    error={sampleSnapshot.error}
                    showTimestamp={showTimestamp}
                    lang={lang}
                    grouping={grouping}
                    repr={repr}
                    channel={channel}
                    scrollRef={numScrollRef}
                    onScroll={handleScroll}
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
            <div className="w-[220px] shrink-0 border-l border-border bg-bg-sidebar flex flex-col overflow-hidden">
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
                  targetTransportLabel={sendTargetLabel}
                />
              </div>
            </div>
          </>
        ) : (
          <div className="flex-1 flex flex-col min-w-0 overflow-hidden">
            {isNum ? (
              <div className="flex-1 flex flex-col min-h-0 overflow-hidden">
                <RawDataViewNumericContent
                  numRows={numRows}
                  status={sampleSnapshot.status}
                  previewSkipped={sampleSnapshot.previewSkipped}
                  retentionEvicted={sampleSnapshot.retentionEvicted}
                  ingressDropped={sampleSnapshot.ingressDropped}
                  error={sampleSnapshot.error}
                  showTimestamp={showTimestamp}
                  lang={lang}
                  grouping={grouping}
                  repr={repr}
                  channel={channel}
                  scrollRef={numScrollRef}
                  onScroll={handleScroll}
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
              targetTransportLabel={sendTargetLabel}
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
