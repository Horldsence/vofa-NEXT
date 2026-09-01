import { useRef, useEffect, useCallback, useMemo, useState } from 'react';
import type { Dispatch, SetStateAction } from 'react';
import type uPlot from 'uplot';
import 'uplot/dist/uPlot.min.css';
import { useAppStore } from '../../../store/appStore';
import { useSettingsStore } from '../../../store/settingsStore';
import { api } from '../../../lib/tauri/tauri';
import { notify, formatError } from '../../../lib/tauri/notifications';
import { useWaveformDetailBuffer } from '../../../lib/hooks/useWaveformDetailBuffer';
import { waveformWindow, type WaveformWindowCache } from '../../../lib/buffers/dataBuffer';
import { writeTextToClipboard } from '../../../lib/utils/clipboard';
import { save } from '@tauri-apps/plugin-dialog';
import { t } from '../../../i18n';
import type {
  ScopeMeasurements,
  WaveformSeriesSelection,
  WaveformWindow,
  WidgetConfig,
} from '../../../types';
import { getEffectiveChannel, type ScopeAxisConfig } from '../../../types';
import {
  timeBaseToWindowSec,
  applyCoupling,
  computeMeasurements,
} from '../../../lib/utils/scopeUtils';
import { WaveformTimeline } from './WaveformTimeline';
import { WaveformEnvelopeChart } from './WaveformEnvelopeChart';
import { waveformSourceIdOf } from '../../../lib/buffers/sourceManagers';
import {
  computeConnectedInputs, buildSeriesSlots, slotColor, resolveInputArray,
  type ConnectedInput, type SeriesSlot, type TimelineSeriesSpec,
} from './waveformSeries';
import { CursorOverlay } from './WaveformChartCursorOverlay';
import { WaveformCursorReadout } from './WaveformCursorReadout';
import { formatTimeMs } from './wavechartFormatters';
import { absoluteTimeRangeUs } from './waveformChartExport';
import {
  useUplotInit, useWheelZoom, usePanDrag, useCursorHide, useTooltipPos,
  type CursorDisplayOpts,
} from './waveformChartHooks';
import { Copy, Download, Check, X } from 'lucide-react';

interface WaveformChartProps {
  widget: Extract<WidgetConfig, { kind: 'Waveform' }>;
  axisConfig: ScopeAxisConfig;
  onConfigChange?: (next: ScopeAxisConfig) => void;
  /// 数据源缓冲 (按 Protocol 源节点溯源); 缺省 = 主波形源单例
  buffer?: WaveformWindowCache;
  /// 当前视图溯源到的 Protocol 节点；detail 订阅以视图为粒度建立。
  sourceId?: string | null;
  onMeasurements?: (key: string, measurements: ScopeMeasurements | null) => void;
}

function buildRawClipboardCsv(
  windowData: WaveformWindow,
  selection: WaveformSeriesSelection,
  widgetId: string,
): string {
  const csvCell = (value: number | undefined) =>
    value !== undefined && Number.isFinite(value) ? String(value) : '';
  const channels = selection.channels;
  const derived = selection.derived;
  const headers = [
    'timestamp_us',
    ...channels.map((channel) => `CH${channel}`),
    ...derived.map((series) =>
      `${series.sink_id}:${series.source_id}:${series.source_handle}`,
    ),
  ];
  const rows = [headers.join(',')];
  const latestUs = windowData.latest_timestamp_us;
  for (let index = 0; index < windowData.timestamps.length; index++) {
    const timestampUs = Math.round(latestUs + windowData.timestamps[index] * 1000);
    rows.push([
      String(timestampUs),
      ...channels.map((channel) => csvCell(windowData.channels[channel]?.[index])),
      ...derived.map((series) => csvCell(
        windowData.derived[series.sink_id]?.[series.source_id]?.[series.source_handle]?.[index]
          ?? windowData.derived[widgetId]?.[series.source_id]?.[series.source_handle]?.[index]
          ?? NaN,
      )),
    ].join(','));
  }
  return rows.join('\n');
}

/// 示波器风格波形图 — 每通道独立 V/div 与 position
/// - 水平: 时基 (sec/div) × 10 格 = 总显示时长
/// - 垂直: V/div × 8 格 (上下各 4 格), 数据归一化到 div
/// - Run/Stop: 停止时冻结数据
/// - 游标: SVG 叠加
/// - 时基与下方缩略图双向同步 (由 WaveformTimeline 实现)
export function WaveformChart({
  widget,
  axisConfig,
  onConfigChange,
  buffer = waveformWindow,
  sourceId = null,
  onMeasurements,
}: WaveformChartProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const plotRef = useRef<uPlot | null>(null);
  const themeId = useSettingsStore((s) => s.settings.appearance.theme);
  // 光标/十字线/采样点显示行为 (全局设置, editor 分类)
  const cursorSnap = useSettingsStore((s) => s.settings.editor.cursorSnap);
  const crosshairVisible = useSettingsStore((s) => s.settings.editor.crosshairVisible);
  const hoverPointsVisible = useSettingsStore((s) => s.settings.editor.hoverPointsVisible);
  const cursorReadoutVisible = useSettingsStore((s) => s.settings.editor.cursorReadoutVisible);
  const waveformFps = useSettingsStore((s) => s.settings.editor.waveformFps);
  const axisConfigRef = useRef(axisConfig);
  const lastVersionRef = useRef(-1);
  const [plotWidth, setPlotWidth] = useState(1000);

  const [cursorReadout, setCursorReadout] = useState<{
    leftPx: number;
    topPx: number;
    xSec: number;
    yDiv: number;
    yVal: number;
    yUnit: string;
    channels: { label: string; val: number; color: string; isDerived: boolean }[];
  } | null>(null);

  const [selectedRange, setSelectedRange] = useState<{ startSec: number; endSec: number } | null>(null);
  const selectionAnchorUsRef = useRef(0);
  const [copyFeedback, setCopyFeedback] = useState(false);

  /// 包络模式 (原型): 后端逐列 min/max 降采样, 前端仅绘制缎带 — 默认关
  const [envelopeMode, setEnvelopeMode] = useState(false);
  const envelopeSourceId = useMemo(() => waveformSourceIdOf(buffer), [buffer]);

  const tooltipRef = useRef<HTMLDivElement>(null);

  const lang = useAppStore((s) => s.lang);
  const rfEdges = useAppStore((s) => s.rfEdges);
  const updateWidget = useAppStore((s) => s.updateWidget);

  // hPosition=0 即实时 (end=0); 运行中也可 >0 回看历史
  const viewEndSec = -axisConfig.hPosition;
  const timeWindowSec = timeBaseToWindowSec(axisConfig.timeBase);

  axisConfigRef.current = axisConfig;

  const connectedInputs = useMemo<ConnectedInput[]>(
    () => computeConnectedInputs(widget.params.id, widget.params.channels, rfEdges),
    [rfEdges, widget.params.id, widget.params.channels]
  );

  const connectedChannels = useMemo(
    () => connectedInputs
      .filter((i): i is Extract<ConnectedInput, { kind: 'channel' }> => i.kind === 'channel')
      .map((i) => i.idx),
    [connectedInputs]
  );

  const seriesSlots = useMemo<SeriesSlot[]>(
    () => buildSeriesSlots(
      connectedInputs,
      widget.params.channels,
      widget.params.dynamicSeries ?? false
    ),
    [connectedInputs, widget.params.channels, widget.params.dynamicSeries]
  );

  const isConnected = widget.params.id === 'default-waveform' || connectedInputs.length > 0;

  const seriesSignature = useMemo(
    () => seriesSlots.map((s) => `${s.isDerived ? 'd' : 'c'}${s.label}`).join(','),
    [seriesSlots]
  );

  const seriesSlotsRef = useRef(seriesSlots);
  seriesSlotsRef.current = seriesSlots;

  /// 缩略图 series — 与主图完全一致的单一数据源:
  /// 相同 slots + axisConfig.channels[].show 可见性过滤 + 相同颜色
  const timelineSeries = useMemo<TimelineSeriesSpec[]>(
    () => seriesSlots
      .filter((s) => axisConfig.channels[s.cfgIdx]?.show ?? true)
      .map((s) => ({ input: s.input, cfgIdx: s.cfgIdx, color: slotColor(s) })),
    [seriesSlots, axisConfig.channels]
  );

  const detailSelection = useMemo<WaveformSeriesSelection>(() => {
    const visible = seriesSlots.filter(
      (slot) => axisConfig.channels[slot.cfgIdx]?.show ?? true,
    );
    const channels = new Set<number>();
    const derived = new Map<string, {
      sink_id: string;
      source_id: string;
      source_handle: string;
    }>();
    for (const slot of visible) {
      if (slot.input.kind === 'channel') {
        channels.add(slot.input.idx);
      } else {
        const item = {
          sink_id: widget.params.id,
          source_id: slot.input.sourceId,
          source_handle: slot.input.sourceHandle,
        };
        derived.set(
          `${item.sink_id}\u0000${item.source_id}\u0000${item.source_handle}`,
          item,
        );
      }
    }
    return {
      channels: [...channels],
      derived: [...derived.values()],
    };
  }, [axisConfig.channels, seriesSlots, widget.params.id]);

  const visibleSeriesCount = Math.max(
    1,
    detailSelection.channels.length + detailSelection.derived.length,
  );
  const detailPointLimit = Math.min(widget.params.max_points, 12_000);
  const detailPointBudget = Math.max(
    2,
    Math.min(
      detailPointLimit,
      Math.max(1000, Math.round(2 * plotWidth * visibleSeriesCount)),
    ),
  );
  const {
    detailBuffer,
    overviewBuffer,
    snapshotId,
    snapshotError,
  } = useWaveformDetailBuffer({
    sourceId,
    running: axisConfig.running,
    viewEndMs: viewEndSec * 1000,
    viewSpanMs: timeWindowSec * 1000,
    pointBudget: detailPointBudget,
    intervalMs: Math.max(1, Math.round(1000 / Math.max(1, waveformFps))),
    selection: detailSelection,
    overviewBuffer: buffer,
  });
  const detailBufferRef = useRef(detailBuffer);
  detailBufferRef.current = detailBuffer;
  const setSelectedRangeAnchored = useCallback<Dispatch<SetStateAction<{
    startSec: number;
    endSec: number;
  } | null>>>((next) => {
    setSelectedRange((previous) => {
      const resolved = typeof next === 'function' ? next(previous) : next;
      selectionAnchorUsRef.current = resolved
        ? detailBufferRef.current.get().latest_timestamp_us
        : 0;
      return resolved;
    });
  }, []);

  useEffect(() => {
    const element = containerRef.current;
    if (!element) return;
    const observer = new ResizeObserver((entries) => {
      const width = Math.max(1, Math.round(entries[0]?.contentRect.width ?? 1));
      setPlotWidth((current) => current === width ? current : width);
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const { cursorHidden, isMac } = useCursorHide();

  // 光标吸附运行时配置: snap 来自设置, hidden 来自 Ctrl/Cmd 隐藏 (隐藏时不吸附但保留十字线)
  const cursorOptsRef = useRef<CursorDisplayOpts>({ snap: cursorSnap, hidden: false });
  cursorOptsRef.current = { snap: cursorSnap, hidden: cursorHidden };

  // 渲染签名: lineMode/pointMode 变化时重建 uPlot series (paths/points), 其它配置变化不重建
  const renderSignature = useMemo(
    () => axisConfig.channels
      .map((c) => `${c.render?.lineMode ?? 'linear'}/${c.render?.pointMode ?? 'none'}`)
      .join(','),
    [axisConfig.channels]
  );

  const tooltipPos = useTooltipPos(cursorReadout, containerRef, tooltipRef);

  /// 取数据 — 返回 [timestamps, ...seriesSlots.length 个等长数组]
  /// 通道与派生输入统一按 SeriesSlot 的完整身份解析。
  /// 未连接的占位槽填 NaN
  const getDisplayData = useCallback((): number[][] => {
    const cfg = axisConfigRef.current;
    const slots = seriesSlotsRef.current;
    const totalSlots = slots.length;
    const win = detailBuffer.get();
    if (win.timestamps.length === 0) {
      return [[0], ...Array.from({ length: totalSlots }, () => [NaN])];
    }
    const timestamps = win.timestamps;
    const channelArrays = win.channels;
    const derivedMap = win.derived;

    const tsLen = timestamps.length;
    // 为每个 slot 构建 data array (与缩略图共用 resolveInputArray, 取数逻辑一致)
    const seriesArrays = slots.map((slot) =>
      resolveInputArray(slot.input, widget.params.id, tsLen, channelArrays, derivedMap)
    );

    const tsSec = timestamps.map((ms) => ms / 1000);
    const seriesDivs = seriesArrays.map((arr, i) => {
      const slot = slots[i];
      const chCfg = getEffectiveChannel(cfg, slot.cfgIdx);
      // 耦合方式 (DC/AC/GND) 先作用于原始数据, 再归一化
      const coupled = applyCoupling(arr, chCfg.coupling);
      const vPerDiv = chCfg.vPerDiv;
      const pos = chCfg.position;
      // sharedY=true: 不归一化, 直接画真实值 (Y 轴 range 用真实值)
      // sharedY=false: 归一化到 div (Y 轴 range 用 [-4, 4] div)
      if (cfg.sharedY) return coupled;
      return coupled.map((v) => (isNaN(v) ? NaN : (v - pos) / vPerDiv));
    });
    return [tsSec, ...seriesDivs];
  }, [widget.params.id, detailBuffer]);

  const updateMeasurements = useCallback(() => {
    if (!onMeasurements) return;
    const win = detailBuffer.get();
    const slot = seriesSlotsRef.current.find(
      (candidate) => axisConfigRef.current.channels[candidate.cfgIdx]?.show ?? true,
    );
    const slotKey = slot?.input.kind === 'channel'
      ? `channel:${slot.input.idx}`
      : `derived:${slot?.input.sourceId ?? ''}:${slot?.input.sourceHandle ?? ''}`;
    if (!slot || win.timestamps.length < 2) {
      onMeasurements(`${detailBuffer.version}:${slotKey}:none`, null);
      return;
    }
    const values = resolveInputArray(
      slot.input,
      widget.params.id,
      win.timestamps.length,
      win.channels,
      win.derived,
    );
    const effective = getEffectiveChannel(axisConfigRef.current, slot.cfgIdx);
    onMeasurements(
      `${detailBuffer.version}:${slotKey}:${effective.coupling}`,
      computeMeasurements(applyCoupling(values, effective.coupling), win.timestamps),
    );
  }, [detailBuffer, onMeasurements, widget.params.id]);

  // 配置变化 → 更新通道可见性 + 重新归一化数据
  // 关键: V/div 或 position 变化时, 必须重新 setData, 否则波形不会按新档位重绘
  // 仅监听会改变数据映射的字段, timeBase/hPosition/cursors 由其他 effect 处理
  const channelConfig = axisConfig.channels;
  useEffect(() => {
    const plot = plotRef.current;
    if (!plot) return;
    const slots = seriesSlotsRef.current;
    for (let i = 0; i < slots.length; i++) {
      plot.setSeries(i + 1, { show: channelConfig[slots[i].cfgIdx]?.show ?? true });
    }
    // 重新归一化数据 (用新的 vPerDiv / position / sharedY / yUnit 重新计算)
    plot.setData(getDisplayData() as unknown as uPlot.AlignedData);
    plot.redraw();
    updateMeasurements();
  }, [
    channelConfig,
    axisConfig.sharedY,
    axisConfig.yUnit,
    seriesSlots,
    getDisplayData,
    updateMeasurements,
  ]);

  useUplotInit(
    containerRef, plotRef, axisConfigRef, seriesSlotsRef,
    getDisplayData, setCursorReadout, setSelectedRangeAnchored,
    seriesSignature, themeId, cursorOptsRef, renderSignature,
  );

  // detail 更新（Run 推送 / Stop 快照查询）— 事件驱动 + rAF 节流
  // waveformWindow.subscribe 在数据到达时触发, 用 rAF 合并多次更新避免超过渲染帧率
  useEffect(() => {
    let rafId: number | null = null;
    const unsub = detailBuffer.subscribe(() => {
      // 数据到达, 如果已有待渲染帧则跳过 (节流)
      if (rafId !== null) return;
      rafId = requestAnimationFrame(() => {
        rafId = null;
        if (plotRef.current) {
          const v = detailBuffer.version;
          if (v !== lastVersionRef.current) {
            lastVersionRef.current = v;
            plotRef.current.setData(getDisplayData() as unknown as uPlot.AlignedData);
            updateMeasurements();
          }
        }
      });
    });
    return () => {
      unsub();
      if (rafId !== null) cancelAnimationFrame(rafId);
    };
  }, [getDisplayData, detailBuffer, updateMeasurements]);

  // 视图同步: timeBase/hPosition 变化时强制 setScale
  useEffect(() => {
    const plot = plotRef.current;
    if (!plot) return;
    plot.setScale('x', { min: viewEndSec - timeWindowSec, max: viewEndSec });
  }, [axisConfig.timeBase, axisConfig.hPosition, axisConfig.running, timeWindowSec, viewEndSec]);

  useWheelZoom(containerRef, axisConfigRef, onConfigChange);

  usePanDrag(containerRef, plotRef, axisConfigRef, onConfigChange, setSelectedRangeAnchored);

  const getAbsoluteSelection = useCallback(() => {
    if (!selectedRange) return null;
    const latestUs = selectionAnchorUsRef.current;
    if (!latestUs) return null;
    return absoluteTimeRangeUs(selectedRange, latestUs);
  }, [selectedRange]);

  const exportSelection = useCallback(() => {
    const absolute = getAbsoluteSelection();
    if (!absolute || !sourceId) return;
    void save({
      defaultPath: 'waveform.csv',
      filters: [{ name: 'CSV', extensions: ['csv'] }],
    }).then(async (path) => {
      if (!path) return;
      try {
        const rows = await api.exportWaveformCsv(
          sourceId,
          snapshotId,
          absolute.startUs,
          absolute.endUs,
          detailSelection,
          path,
        );
        notify.info('CSV', t(lang, 'waveformCsvExported').replace('{{count}}', String(rows)));
      } catch (error) {
        notify.error('CSV', formatError(error));
      }
    });
  }, [detailSelection, getAbsoluteSelection, lang, snapshotId, sourceId]);

  const copySelection = useCallback(async () => {
    const absolute = getAbsoluteSelection();
    if (!absolute || !sourceId) return;
    try {
      const raw = await api.getWaveformRawRange(
        sourceId,
        snapshotId,
        absolute.startUs,
        absolute.endUs,
        100_000,
        detailSelection,
      );
      const csv = buildRawClipboardCsv(raw, detailSelection, widget.params.id);
      const ok = await writeTextToClipboard(csv);
      if (ok) {
        setCopyFeedback(true);
        setTimeout(() => setCopyFeedback(false), 1200);
      }
    } catch (error) {
      notify.warn(t(lang, 'waveformCopyTitle'), formatError(error));
    }
  }, [detailSelection, getAbsoluteSelection, lang, snapshotId, sourceId, widget.params.id]);

  const clearSelection = useCallback(() => {
    setSelectedRange(null);
    plotRef.current?.setSelect({ left: 0, top: 0, width: 0, height: 0 }, false);
  }, []);

  return (
    <div className="flex flex-col h-full w-full" style={{ flexDirection: 'column' }}>
      <div
        className={`waveform-container ${cursorHidden ? 'cursor-hidden' : ''} ${crosshairVisible ? '' : 'crosshair-hidden'} ${hoverPointsVisible ? '' : 'hoverpoint-hidden'} ${cursorSnap && !cursorHidden ? 'snap-on' : ''} flex-1 min-h-0 relative`}
        ref={containerRef}
        onMouseLeave={() => setCursorReadout(null)}
      >
        {axisConfig.cursors.enabled && (
          <svg className="absolute inset-0 pointer-events-none z-5">
            <CursorOverlay
              cursors={axisConfig.cursors}
              running={axisConfig.running}
              hPosition={axisConfig.hPosition}
              timeWindowSec={timeWindowSec}
              connectedChannels={connectedChannels}
              sharedY={axisConfig.sharedY}
              channels={axisConfig.channels}
            />
          </svg>
        )}

        {/* 包络模式开关 (右上角, 提示条下方) — 开启时覆盖绘制后端降采样缎带 */}
        {envelopeSourceId && (
          <button
            className={`absolute top-7 right-2 z-[100] px-2 py-0.5 text-[10px] rounded border transition-colors select-none ${
              envelopeMode
                ? 'text-green bg-bg-editor border-green/50'
                : 'text-text-secondary bg-bg-editor/95 border-border/30 hover:text-text-primary hover:bg-bg-hover'
            }`}
            title={t(lang, 'envelopeModeTitle')}
            onClick={() => setEnvelopeMode((v) => !v)}
          >
            {t(lang, 'envelopeMode')}
          </button>
        )}
        {envelopeMode && envelopeSourceId && (
          <div className="absolute inset-0 z-50 bg-bg-editor">
            <WaveformEnvelopeChart
              sourceId={envelopeSourceId}
              series={timelineSeries.map((s) => ({
                label:
                  s.input.kind === 'channel'
                    ? `CH${s.input.idx + 1}`
                    : `MATH:${s.input.sourceId}`,
                color: s.color,
              }))}
            />
          </div>
        )}

        {/* 左上角提示: 按住 Ctrl/Cmd 隐藏光标 */}
        <div className="absolute top-1.5 right-2 z-[100] px-2 py-0.5 text-[10px] text-text-primary bg-bg-editor/95 border border-border/30 rounded pointer-events-none select-none shadow whitespace-nowrap">
          {cursorHidden
            ? t(lang, 'cursorHiddenHint')
            : (isMac ? '⌘ ' : 'Ctrl ') + t(lang, 'cursorHideHint')}
        </div>

        {/* 框选导出/复制工具栏 */}
        {selectedRange && (
          <div className="absolute top-9 right-2 z-[100] flex items-center gap-1 px-1.5 py-1 bg-bg-editor/95 border border-border/30 rounded shadow-lg select-none">
            <span className="text-[10px] text-text-secondary font-mono px-1">
              {formatTimeMs(selectedRange.startSec * 1000)} - {formatTimeMs(selectedRange.endSec * 1000)}
            </span>
            <button
              className="p-1 text-text-secondary hover:text-text-primary hover:bg-bg-hover rounded transition-colors"
              title={t(lang, 'copySelection')}
              onClick={() => { void copySelection(); }}
            >
              {copyFeedback ? <Check size={12} className="text-green" /> : <Copy size={12} />}
            </button>
            <button
              className="p-1 text-text-secondary hover:text-text-primary hover:bg-bg-hover rounded transition-colors"
              title={t(lang, 'exportSelection')}
              onClick={exportSelection}
            >
              <Download size={12} />
            </button>
            <button
              className="p-1 text-text-secondary hover:text-text-primary hover:bg-bg-hover rounded transition-colors"
              title={t(lang, 'clearSelection')}
              onClick={clearSelection}
            >
              <X size={12} />
            </button>
          </div>
        )}

        {/* 右下角动态 series 开关 (仅用户创建的波形图, default-waveform 不显示) */}
        {widget.params.id !== 'default-waveform' && (
          <button
            className={`absolute bottom-1.5 right-2 z-[100] px-2.5 py-0.5 text-[10px] text-text-primary bg-bg-editor/95 border border-border/30 rounded cursor-pointer select-none shadow whitespace-nowrap transition-all duration-150 hover:bg-bg-hover hover:border-border/50 ${widget.params.dynamicSeries ? 'text-orange border-orange/50 bg-orange/10' : ''}`}
            onClick={() => {
              updateWidget(widget.params.id, {
                ...widget,
                params: {
                  ...widget.params,
                  dynamicSeries: !widget.params.dynamicSeries,
                },
              });
            }}
            title={t(lang, 'dynamicSeriesToggle')}
          >
            {widget.params.dynamicSeries
              ? t(lang, 'dynamicSeriesOn')
              : t(lang, 'dynamicSeriesOff')}
          </button>
        )}

        <WaveformCursorReadout
          readout={cursorReadout}
          hidden={!cursorReadoutVisible || cursorHidden}
          tooltipPos={tooltipPos}
          tooltipRef={tooltipRef}
        />
        {snapshotError && !axisConfig.running && (
          <div className="absolute bottom-2 left-2 z-[100] max-w-[70%] rounded border border-orange/50 bg-bg-editor/95 px-2 py-1 text-[10px] text-orange shadow">
            {t(lang, 'waveformSnapshotUnavailable')}: {snapshotError}
          </div>
        )}
      </div>
      {!isConnected && (
        <div className="absolute inset-0 flex items-center justify-center text-text-secondary text-sm pointer-events-none">
          <span>{t(lang, 'emptyWaveform')}</span>
        </div>
      )}
      <WaveformTimeline
        axisConfig={axisConfig}
        viewEndSec={viewEndSec}
        timeWindowSec={timeWindowSec}
        series={timelineSeries}
        widgetId={widget.params.id}
        buffer={overviewBuffer}
        onConfigChange={onConfigChange}
      />
    </div>
  );
}
