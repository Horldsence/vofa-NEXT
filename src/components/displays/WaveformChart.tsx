import { useRef, useEffect, useCallback, useMemo, useState } from 'react';
import uPlot from 'uplot';
import 'uplot/dist/uPlot.min.css';
import { useAppStore } from '../../store/appStore';
import { useSettingsStore } from '../../store/settingsStore';
import { waveformWindow } from '../../lib/dataBuffer';
import { writeTextToClipboard } from '../../lib/clipboard';
import { t } from '../../i18n';
import type { WidgetConfig } from '../../types';
import { getEffectiveChannel, type ScopeAxisConfig } from '../../types';
import { timeBaseToWindowSec } from '../../lib/scopeUtils';
import { WaveformTimeline } from './WaveformTimeline';
import {
  computeConnectedInputs, buildSeriesSlots, slotColor, resolveInputArray,
  type ConnectedInput, type SeriesSlot, type FrozenWaveformData, type TimelineSeriesSpec,
} from './waveformSeries';
import { CursorOverlay } from './WaveformChartCursorOverlay';
import { WaveformCursorReadout } from './WaveformCursorReadout';
import { getExportData, buildCsvForRange } from './waveformChartExport';
import { formatTimeMs } from './wavechartFormatters';
import {
  useUplotInit, useWheelZoom, usePanDrag, useCursorHide, useTooltipPos,
} from './waveformChartHooks';
import { Copy, Download, Check, X } from 'lucide-react';

interface WaveformChartProps {
  widget: Extract<WidgetConfig, { kind: 'Waveform' }>;
  axisConfig: ScopeAxisConfig;
  onConfigChange?: (next: ScopeAxisConfig) => void;
}

/// 示波器风格波形图 — 每通道独立 V/div 与 position
/// - 水平: 时基 (sec/div) × 10 格 = 总显示时长
/// - 垂直: V/div × 8 格 (上下各 4 格), 数据归一化到 div
/// - Run/Stop: 停止时冻结数据
/// - 游标: SVG 叠加
/// - 时基与下方缩略图双向同步 (由 WaveformTimeline 实现)
export function WaveformChart({ widget, axisConfig, onConfigChange }: WaveformChartProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const plotRef = useRef<uPlot | null>(null);
  const themeId = useSettingsStore((s) => s.settings.appearance.theme);
  const axisConfigRef = useRef(axisConfig);
  const lastVersionRef = useRef(-1);
  const frozenDataRef = useRef<FrozenWaveformData | null>(null);
  /// 冻结快照 state — 与 frozenDataRef 同步更新, 供渲染期 (缩略图 prop) 使用
  /// 避免渲染期读 ref 时 Stop 后第一帧尚未赋值的问题
  const [frozenData, setFrozenData] = useState<FrozenWaveformData | null>(null);

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
  const [copyFeedback, setCopyFeedback] = useState(false);

  const tooltipRef = useRef<HTMLDivElement>(null);

  const lang = useAppStore((s) => s.lang);
  const rfEdges = useAppStore((s) => s.rfEdges);
  const updateWidget = useAppStore((s) => s.updateWidget);

  const viewEndSec = axisConfig.running ? 0 : -axisConfig.hPosition;
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

  const { cursorHidden, isMac } = useCursorHide();

  const tooltipPos = useTooltipPos(cursorReadout, containerRef, tooltipRef);

  /// 取数据 — 返回 [timestamps, ...seriesSlots.length 个等长数组]
  /// 通道输入: 从 win.channels[idx] 取; 派生输入: 从 win.derived[widgetId]?.[sourceId] 取
  /// 未连接的占位槽填 NaN
  const getDisplayData = useCallback((): number[][] => {
    const cfg = axisConfigRef.current;
    const slots = seriesSlotsRef.current;
    const totalSlots = slots.length;
    let timestamps: number[];
    let channelArrays: number[][];
    let derivedMap: Record<string, Record<string, number[]>> | undefined;

    if (cfg.running) {
      const win = waveformWindow.get();
      if (win.timestamps.length === 0) {
        return [[0], ...Array.from({ length: totalSlots }, () => [NaN])];
      }
      timestamps = win.timestamps;
      channelArrays = win.channels;
      derivedMap = win.derived;
    } else {
      const frozen = frozenDataRef.current;
      if (!frozen || frozen.ts.length === 0) {
        return [[0], ...Array.from({ length: totalSlots }, () => [NaN])];
      }
      timestamps = frozen.ts;
      channelArrays = frozen.chs;
      derivedMap = frozen.derived;
    }

    const tsLen = timestamps.length;
    // 为每个 slot 构建 data array (与缩略图共用 resolveInputArray, 取数逻辑一致)
    const seriesArrays = slots.map((slot) =>
      resolveInputArray(slot.input, widget.params.id, tsLen, channelArrays, derivedMap)
    );

    const tsSec = timestamps.map((ms) => ms / 1000);
    const seriesDivs = seriesArrays.map((arr, i) => {
      const slot = slots[i];
      const chCfg = getEffectiveChannel(cfg, slot.cfgIdx);
      const vPerDiv = chCfg.vPerDiv;
      const pos = chCfg.position;
      // sharedY=true: 不归一化, 直接画真实值 (Y 轴 range 用真实值)
      // sharedY=false: 归一化到 div (Y 轴 range 用 [-4, 4] div)
      if (cfg.sharedY) return arr;
      return arr.map((v) => (isNaN(v) ? NaN : (v - pos) / vPerDiv));
    });
    return [tsSec, ...seriesDivs];
  }, [widget.params.channels, widget.params.id]);

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
  }, [channelConfig, axisConfig.sharedY, axisConfig.yUnit, seriesSlots, getDisplayData]);

  useEffect(() => {
    if (!axisConfig.running) {
      const win = waveformWindow.get();
      if (win.timestamps.length > 0 && !frozenDataRef.current) {
        // 停止时保持引用而非深拷贝: waveformWindow.clear() 创建新空窗口,
        // 不会 mutate 此引用, 因此引用安全
        const snapshot: FrozenWaveformData = {
          ts: win.timestamps,
          chs: win.channels,
          derived: win.derived,
        };
        frozenDataRef.current = snapshot;
        // 同步 setState, 保证缩略图在 Stop 的同一帧拿到冻结快照
        setFrozenData(snapshot);
        // 拍快照后立即用冻结数据重绘
        const plot = plotRef.current;
        if (plot) {
          plot.setData(getDisplayData() as unknown as uPlot.AlignedData);
          plot.redraw();
        }
      }
    } else {
      frozenDataRef.current = null;
      setFrozenData(null);
    }
  }, [axisConfig.running, getDisplayData]);

  useUplotInit(
    containerRef, plotRef, axisConfigRef, seriesSlotsRef,
    getDisplayData, setCursorReadout, setSelectedRange,
    seriesSignature, themeId,
  );

  // 数据更新 (运行模式) — 事件驱动 + rAF 节流
  // waveformWindow.subscribe 在数据到达时触发, 用 rAF 合并多次更新避免超过渲染帧率
  useEffect(() => {
    if (!axisConfig.running) return;
    let rafId: number | null = null;
    const unsub = waveformWindow.subscribe(() => {
      // 数据到达, 如果已有待渲染帧则跳过 (节流)
      if (rafId !== null) return;
      rafId = requestAnimationFrame(() => {
        rafId = null;
        if (plotRef.current) {
          const v = waveformWindow.version;
          if (v !== lastVersionRef.current) {
            lastVersionRef.current = v;
            plotRef.current.setData(getDisplayData() as unknown as uPlot.AlignedData);
          }
        }
      });
    });
    return () => {
      unsub();
      if (rafId !== null) cancelAnimationFrame(rafId);
    };
  }, [getDisplayData, axisConfig.running]);

  // 视图同步: timeBase/hPosition 变化时强制 setScale
  useEffect(() => {
    const plot = plotRef.current;
    if (!plot) return;
    if (axisConfig.running) {
      plot.setScale('x', { min: -timeWindowSec, max: 0 });
    } else {
      plot.setScale('x', { min: viewEndSec - timeWindowSec, max: viewEndSec });
    }
  }, [axisConfig.timeBase, axisConfig.hPosition, axisConfig.running, timeWindowSec, viewEndSec]);

  useWheelZoom(containerRef, axisConfigRef, onConfigChange);

  usePanDrag(containerRef, plotRef, axisConfigRef, onConfigChange);

  const exportSelection = useCallback(() => {
    if (!selectedRange) return;
    const data = getExportData(
      axisConfigRef.current,
      seriesSlotsRef.current,
      widget.params.id,
      frozenDataRef.current,
    );
    const csv = buildCsvForRange(selectedRange, data);
    if (!csv) return;
    const blob = new Blob([csv], { type: 'text/csv;charset=utf-8;' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `waveform-${selectedRange.startSec.toFixed(3)}-${selectedRange.endSec.toFixed(3)}.csv`;
    a.click();
    URL.revokeObjectURL(url);
  }, [selectedRange, widget.params.id]);

  const copySelection = useCallback(async () => {
    if (!selectedRange) return;
    const data = getExportData(
      axisConfigRef.current,
      seriesSlotsRef.current,
      widget.params.id,
      frozenDataRef.current,
    );
    const csv = buildCsvForRange(selectedRange, data);
    if (!csv) return;
    const ok = await writeTextToClipboard(csv);
    if (ok) {
      setCopyFeedback(true);
      setTimeout(() => setCopyFeedback(false), 1200);
    }
  }, [selectedRange, widget.params.id]);

  const clearSelection = useCallback(() => {
    setSelectedRange(null);
    plotRef.current?.setSelect({ left: 0, top: 0, width: 0, height: 0 }, false);
  }, []);

  return (
    <div className="flex flex-col h-full w-full" style={{ flexDirection: 'column' }}>
      <div
        className={`waveform-container ${cursorHidden ? 'cursor-hidden' : ''} flex-1 min-h-0 relative`}
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
              onClick={copySelection}
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
          hidden={cursorHidden}
          tooltipPos={tooltipPos}
          tooltipRef={tooltipRef}
        />
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
        frozenData={!axisConfig.running ? frozenData : null}
        onConfigChange={onConfigChange}
      />
    </div>
  );
}

