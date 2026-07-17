import { useRef, useEffect, useLayoutEffect, useCallback, useMemo } from 'react';
import uPlot from 'uplot';
import 'uplot/dist/uPlot.min.css';
import { useAppStore } from '../../store/appStore';
import { waveformWindow } from '../../lib/dataBuffer';
import { t } from '../../i18n';
import type { WidgetConfig } from '../../types';
import { TIME_BASES_SEC, formatVPerDiv, type ScopeAxisConfig } from '../../types';
import { timeBaseToWindowSec, VERTICAL_DIVS } from '../../lib/scopeUtils';
import {
  CHANNEL_COLORS, TEXT_COLOR, GRID_COLOR, TICK_COLOR, CURSOR_COLOR, getContainerSize,
} from './waveformConstants';
import { WaveformTimeline } from './WaveformTimeline';

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
  const axisConfigRef = useRef(axisConfig);
  const lastVersionRef = useRef(-1);
  const frozenDataRef = useRef<{ ts: number[]; chs: number[][] } | null>(null);

  const lang = useAppStore((s) => s.lang);
  const rfEdges = useAppStore((s) => s.rfEdges);

  const viewEndSec = axisConfig.running ? 0 : -axisConfig.hPosition;
  const timeWindowSec = timeBaseToWindowSec(axisConfig.timeBase);

  // 解析连接的通道
  const connectedChannels = useMemo(() => {
    if (widget.params.id === 'default-waveform') {
      const win = waveformWindow.get();
      const count = win.channel_count || widget.params.channels;
      return Array.from({ length: count }, (_, i) => i);
    }
    return rfEdges
      .filter((e) => e.target === widget.params.id)
      .map((e) => {
        const m = /^ch(\d+)$/.exec(e.sourceHandle ?? '');
        return m ? parseInt(m[1], 10) : -1;
      })
      .filter((i) => i >= 0);
  }, [rfEdges, widget.params.id, widget.params.channels]);

  const isConnected = widget.params.id === 'default-waveform' || connectedChannels.length > 0;

  // 缓存 connectedChannels 供 getDisplayData (useCallback) 使用, 避免 effect 重建
  const connectedChannelsRef = useRef(connectedChannels);
  useEffect(() => { connectedChannelsRef.current = connectedChannels; }, [connectedChannels]);

  // 配置变化 → 更新 ref + 通道可见性
  useEffect(() => {
    axisConfigRef.current = axisConfig;
    const plot = plotRef.current;
    if (!plot) return;
    for (let i = 0; i < widget.params.channels; i++) {
      plot.setSeries(i + 1, { show: axisConfig.channels[i]?.show ?? true });
    }
    plot.redraw();
  }, [axisConfig, widget.params.channels]);

  /// 取数据 — 始终返回 widget.params.channels + 1 个等长数组
  /// 未连接的通道填 NaN (不显示), 仅连接的通道有数据
  const getDisplayData = useCallback((): number[][] => {
    const cfg = axisConfigRef.current;
    const totalCh = widget.params.channels;
    let timestamps: number[];
    let channelArrays: number[][];

    if (cfg.running) {
      const win = waveformWindow.get();
      if (win.timestamps.length === 0) {
        return [[0], ...Array.from({ length: totalCh }, () => [NaN])];
      }
      timestamps = win.timestamps;
      channelArrays = padChannels(win.channels, timestamps.length, totalCh);
    } else {
      const frozen = frozenDataRef.current;
      if (!frozen || frozen.ts.length === 0) {
        return [[0], ...Array.from({ length: totalCh }, () => [NaN])];
      }
      timestamps = frozen.ts;
      channelArrays = padChannels(frozen.chs, timestamps.length, totalCh);
    }

    // 按 connectedChannels 过滤: 未连接通道填 NaN
    const connected = connectedChannelsRef.current;
    const connectedSet = new Set(connected);
    const filteredArrays = channelArrays.map((arr, i) =>
      connectedSet.has(i) ? arr : arr.map(() => NaN)
    );

    const tsSec = timestamps.map((ms) => ms / 1000);
    const channelDivs = filteredArrays.map((arr, i) => {
      const chCfg = cfg.channels[i];
      const vPerDiv = chCfg?.vPerDiv ?? 1;
      const pos = chCfg?.position ?? 0;
      return arr.map((v) => (isNaN(v) ? NaN : (v - pos) / vPerDiv));
    });
    return [tsSec, ...channelDivs];
  }, [widget.params.channels]);

  // 冻结快照
  useEffect(() => {
    if (!axisConfig.running) {
      const win = waveformWindow.get();
      if (win.timestamps.length > 0 && !frozenDataRef.current) {
        frozenDataRef.current = {
          ts: [...win.timestamps],
          chs: win.channels.map((ch) => [...ch]),
        };
      }
    } else {
      frozenDataRef.current = null;
    }
  }, [axisConfig.running]);

  // 初始化 uPlot
  useLayoutEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    let plot: uPlot | null = null;
    let resizeRaf: number | null = null;
    let lastW = 0, lastH = 0;

    const createSeries = (): uPlot.Series[] => {
      const series: uPlot.Series[] = [{
        label: 't', stroke: TEXT_COLOR,
        value: (_u, v) => (v == null ? '--' : (v * 1000).toFixed(0) + 'ms'),
      }];
      for (let i = 0; i < widget.params.channels; i++) {
        const chCfg = axisConfigRef.current.channels[i];
        const vPerDiv = chCfg?.vPerDiv ?? 1;
        const pos = chCfg?.position ?? 0;
        series.push({
          label: `CH${i} ${formatVPerDiv(vPerDiv)}`,
          stroke: CHANNEL_COLORS[i % CHANNEL_COLORS.length],
          width: 1.5,
          value: (_u, v) => (v == null ? '--' : (v * vPerDiv + pos).toFixed(3) + 'V'),
          show: chCfg?.show ?? true,
        });
      }
      return series;
    };

    const createOptions = (w: number, h: number): uPlot.Options => {
      const cfg = axisConfigRef.current;
      const gridStroke = cfg.grid ? GRID_COLOR : 'transparent';
      return {
        width: w, height: h, series: createSeries(),
        axes: [
          {
            stroke: TEXT_COLOR, grid: { stroke: gridStroke, width: 1 },
            ticks: { stroke: TICK_COLOR }, size: 28, gap: 4, label: 'ms',
            labelSize: 20, labelFont: '11px sans-serif',
            values: (_self, ticks) => ticks.map((v) => (v * 1000).toFixed(0)),
          },
          {
            stroke: TEXT_COLOR, grid: { stroke: gridStroke, width: 1 },
            ticks: { stroke: TICK_COLOR }, size: 44, gap: 4, label: 'div',
            labelSize: 16, labelFont: '11px sans-serif',
            values: (_self, ticks) => ticks.map((v) => v.toFixed(0)),
          },
        ],
        legend: { show: false },
        cursor: { points: { size: 4 }, drag: { x: true, y: false } },
        scales: {
          x: {
            time: false,
            range: () => {
              const c = axisConfigRef.current;
              const end = c.running ? 0 : -c.hPosition;
              const win = timeBaseToWindowSec(c.timeBase);
              return [end - win, end];
            },
          },
          y: { range: () => [-VERTICAL_DIVS / 2, VERTICAL_DIVS / 2] },
        },
      };
    };

    const createPlot = () => {
      const { w, h } = getContainerSize(container);
      plot = new uPlot(
        createOptions(w, h),
        getDisplayData() as unknown as uPlot.AlignedData,
        container
      );
      plotRef.current = plot;
      lastW = w; lastH = h;
    };

    const resize = () => {
      const { w, h } = getContainerSize(container);
      if (w === lastW && h === lastH) return;
      if (!plot) { createPlot(); return; }
      if (resizeRaf !== null) cancelAnimationFrame(resizeRaf);
      resizeRaf = requestAnimationFrame(() => {
        plot?.setSize({ width: w, height: h });
        lastW = w; lastH = h;
      });
    };

    createPlot();
    const ro = new ResizeObserver(resize);
    ro.observe(container);
    window.addEventListener('resize', resize);
    return () => {
      ro.disconnect();
      window.removeEventListener('resize', resize);
      if (resizeRaf !== null) cancelAnimationFrame(resizeRaf);
      plot?.destroy();
      plotRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [widget.params.channels]);

  // 数据更新 (运行模式)
  useEffect(() => {
    if (!axisConfig.running) return;
    let rafId: number | null = null;
    const tick = () => {
      if (plotRef.current) {
        const v = waveformWindow.version;
        if (v !== lastVersionRef.current) {
          lastVersionRef.current = v;
          plotRef.current.setData(getDisplayData() as unknown as uPlot.AlignedData);
        }
      }
      rafId = requestAnimationFrame(tick);
    };
    rafId = requestAnimationFrame(tick);
    return () => { if (rafId !== null) cancelAnimationFrame(rafId); };
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

  // 滚轮 → 时基档位 (自由拖动后 timeBase 可能不在档位中, 先找最近档再步进)
  const handleWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    e.stopPropagation();
    let idx = TIME_BASES_SEC.indexOf(axisConfig.timeBase);
    if (idx < 0) {
      // 找最近档位作为起点
      let bestDiff = Infinity;
      for (let i = 0; i < TIME_BASES_SEC.length; i++) {
        const d = Math.abs(TIME_BASES_SEC[i] - axisConfig.timeBase);
        if (d < bestDiff) { bestDiff = d; idx = i; }
      }
    }
    const next = Math.max(0, Math.min(TIME_BASES_SEC.length - 1, idx + (e.deltaY > 0 ? 1 : -1)));
    onConfigChange?.({ ...axisConfig, timeBase: TIME_BASES_SEC[next] });
  }, [axisConfig, onConfigChange]);

  // 游标 SVG overlay
  const cursorOverlay = useMemo(() => {
    if (!axisConfig.cursors.enabled) return null;
    const cfg = axisConfig.cursors;
    if (cfg.type === 'vertical') {
      const viewEnd = axisConfig.running ? 0 : -axisConfig.hPosition;
      const viewStart = viewEnd - timeWindowSec;
      const range = timeWindowSec || 1;
      const c1R = (cfg.c1 - viewStart) / range;
      const c2R = (cfg.c2 - viewStart) / range;
      return (
        <>
          <line x1={`${c1R * 100}%`} y1="0" x2={`${c1R * 100}%`} y2="100%" stroke={CURSOR_COLOR} strokeWidth={1} strokeDasharray="4 2" />
          <line x1={`${c2R * 100}%`} y1="0" x2={`${c2R * 100}%`} y2="100%" stroke={CURSOR_COLOR} strokeWidth={1} strokeDasharray="4 2" />
        </>
      );
    }
    const chIdx = connectedChannels[0] ?? 0;
    const chCfg = axisConfig.channels[chIdx];
    const vPerDiv = chCfg?.vPerDiv ?? 1;
    const pos = chCfg?.position ?? 0;
    const c1Div = (cfg.c1 - pos) / vPerDiv;
    const c2Div = (cfg.c2 - pos) / vPerDiv;
    const c1R = 1 - (c1Div + VERTICAL_DIVS / 2) / VERTICAL_DIVS;
    const c2R = 1 - (c2Div + VERTICAL_DIVS / 2) / VERTICAL_DIVS;
    return (
      <>
        <line x1="0" y1={`${c1R * 100}%`} x2="100%" y2={`${c1R * 100}%`} stroke={CURSOR_COLOR} strokeWidth={1} strokeDasharray="4 2" />
        <line x1="0" y1={`${c2R * 100}%`} x2="100%" y2={`${c2R * 100}%`} stroke={CURSOR_COLOR} strokeWidth={1} strokeDasharray="4 2" />
      </>
    );
  }, [axisConfig.cursors, axisConfig.channels, axisConfig.running, axisConfig.hPosition, timeWindowSec, connectedChannels]);

  return (
    <div className="waveform-layout" style={{ flexDirection: 'column' }}>
      <div
        className="waveform-container"
        ref={containerRef}
        onWheel={handleWheel}
        style={{ flex: 1, minHeight: 0, position: 'relative' }}
      >
        {axisConfig.cursors.enabled && (
          <svg className="cursor-overlay" style={{ position: 'absolute', inset: 0, pointerEvents: 'none', zIndex: 5 }}>
            {cursorOverlay}
          </svg>
        )}
      </div>
      {!isConnected && (
        <div className="waveform-empty-overlay">
          <span>{t(lang, 'emptyWaveform')}</span>
        </div>
      )}
      <WaveformTimeline
        axisConfig={axisConfig}
        viewEndSec={viewEndSec}
        timeWindowSec={timeWindowSec}
        connectedChannels={connectedChannels}
        onConfigChange={onConfigChange}
      />
    </div>
  );
}

/// 将每通道数据对齐到 targetLen (短补 NaN, 长截断)
function padChannels(channels: number[][], targetLen: number, totalCh: number): number[][] {
  return Array.from({ length: totalCh }, (_, idx) => {
    const ch = channels[idx];
    if (!ch) return Array(targetLen).fill(NaN);
    if (ch.length === targetLen) return ch;
    const padded = ch.slice(0, targetLen);
    while (padded.length < targetLen) padded.push(NaN);
    return padded;
  });
}
