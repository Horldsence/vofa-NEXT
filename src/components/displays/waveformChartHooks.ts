import { useRef, useEffect, useLayoutEffect, useMemo, useState } from 'react';
import uPlot from 'uplot';
import { timeBaseToWindowSec, VERTICAL_DIVS } from '../../lib/scopeUtils';
import { getEffectiveChannel, TIME_BASES_SEC, V_PER_DIV, formatVPerDiv } from '../../types';
import {
  CHANNEL_COLORS, DERIVED_COLORS, TEXT_COLOR, GRID_COLOR, TICK_COLOR, getContainerSize,
} from './waveformConstants';
import { getThemeColor, formatTimeMs, formatYValue } from './wavechartFormatters';
import type { ScopeAxisConfig } from '../../types';
import type { SeriesSlot } from './waveformSeries';

// ---- uPlot 初始化 ----

export function useUplotInit(
  containerRef: React.RefObject<HTMLDivElement | null>,
  plotRef: React.MutableRefObject<uPlot | null>,
  axisConfigRef: React.MutableRefObject<ScopeAxisConfig>,
  seriesSlotsRef: React.MutableRefObject<SeriesSlot[]>,
  getDisplayData: () => number[][],
  setCursorReadout: React.Dispatch<React.SetStateAction<{
    leftPx: number; topPx: number; xSec: number; yDiv: number;
    yVal: number; yUnit: string;
    channels: { label: string; val: number; color: string; isDerived: boolean }[];
  } | null>>,
  setSelectedRange: React.Dispatch<React.SetStateAction<{ startSec: number; endSec: number } | null>>,
  seriesSignature: string,
  themeId: string,
) {
  useLayoutEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    let plot: uPlot | null = null;
    let resizeRaf: number | null = null;
    let lastW = 0, lastH = 0;

    const createSeries = (): uPlot.Series[] => {
      const cfg0 = axisConfigRef.current;
      const yUnit = cfg0.yUnit ?? '';
      const slots = seriesSlotsRef.current;
      const textColor = getThemeColor('--color-waveform-text', TEXT_COLOR);
      const series: uPlot.Series[] = [{
        label: 't', stroke: textColor,
        value: (_u, v) => (v == null ? '--' : formatTimeMs(v * 1000)),
      }];
      for (const slot of slots) {
        const chCfg = getEffectiveChannel(cfg0, slot.cfgIdx);
        const vPerDiv = chCfg.vPerDiv;
        const pos = chCfg.position;
        const show = cfg0.channels[slot.cfgIdx]?.show ?? true;
        const color = slot.isDerived
          ? DERIVED_COLORS[slot.colorIdx % DERIVED_COLORS.length]
          : CHANNEL_COLORS[slot.colorIdx % CHANNEL_COLORS.length];
        series.push({
          label: `${slot.label} ${formatVPerDiv(vPerDiv, yUnit)}`,
          stroke: color,
          width: 1.5,
          value: (_u, v) => {
            if (v == null) return '--';
            const c = axisConfigRef.current;
            const real = c.sharedY ? v : v * vPerDiv + pos;
            return formatYValue(real, c.yUnit ?? '');
          },
          show,
        });
      }
      return series;
    };

    const createOptions = (w: number, h: number): uPlot.Options => {
      const cfg = axisConfigRef.current;
      const gridStroke = cfg.grid ? getThemeColor('--color-waveform-grid', GRID_COLOR) : 'transparent';
      const textColor = getThemeColor('--color-waveform-text', TEXT_COLOR);
      const tickColor = getThemeColor('--color-waveform-tick', TICK_COLOR);
      return {
        width: w, height: h, series: createSeries(),
        axes: [
          {
            stroke: textColor, grid: { stroke: gridStroke, width: 1 },
            ticks: { stroke: tickColor }, size: 32, gap: 4,
            label: '',
            labelSize: 20, labelFont: '11px sans-serif',
            values: (_self, ticks) => {
              const c = axisConfigRef.current;
              const winSec = timeBaseToWindowSec(c.timeBase);
              if (winSec >= 1) return ticks.map((v) => v.toFixed(2) + 's');
              if (winSec >= 0.001) return ticks.map((v) => (v * 1000).toFixed(1) + 'ms');
              return ticks.map((v) => (v * 1e6).toFixed(0) + 'µs');
            },
          },
          {
            stroke: textColor, grid: { stroke: gridStroke, width: 1 },
            ticks: { stroke: tickColor }, size: 50, gap: 4,
            label: '',
            labelSize: 16, labelFont: '11px sans-serif',
            values: (_self, ticks) => {
              const c = axisConfigRef.current;
              if (c.sharedY) {
                const unit = c.yUnit ?? '';
                return ticks.map((v) => formatYValue(v, unit));
              }
              return ticks.map((v) => v.toFixed(0));
            },
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
          y: {
            range: () => {
              const c = axisConfigRef.current;
              if (c.sharedY) {
                const ch0 = getEffectiveChannel(c, 0);
                return [
                  ch0.position - (ch0.vPerDiv * VERTICAL_DIVS) / 2,
                  ch0.position + (ch0.vPerDiv * VERTICAL_DIVS) / 2,
                ];
              }
              return [-VERTICAL_DIVS / 2, VERTICAL_DIVS / 2];
            },
          },
        },
        hooks: {
          setCursor: [
            (u: uPlot) => {
              const idx = u.cursor.idx;
              const left = u.cursor.left;
              const top = u.cursor.top;
              if (idx == null || left == null || top == null || idx < 0 || left < 0 || top < 0) {
                setCursorReadout(null);
                return;
              }
              const c = axisConfigRef.current;
              const xSec = u.posToVal(left, 'x');
              const yPixelVal = u.posToVal(top, 'y');
              const bbox = (u as unknown as { bbox?: { left: number; top: number } }).bbox;
              const plotLeft = bbox?.left ?? 0;
              const plotTop = bbox?.top ?? 0;
              const canvasLeft = left + plotLeft;
              const canvasTop = top + plotTop;
              const slots = seriesSlotsRef.current;
              const firstVisibleIdx = slots.findIndex((s) => c.channels[s.cfgIdx]?.show ?? true);
              const slotIdx = firstVisibleIdx >= 0 ? firstVisibleIdx : 0;
              const firstSlot = slots[slotIdx];
              const chCfg = firstSlot
                ? getEffectiveChannel(c, firstSlot.cfgIdx)
                : { vPerDiv: 1, position: 0 };
              const yVal = c.sharedY ? yPixelVal : yPixelVal * chCfg.vPerDiv + chCfg.position;
              const channels: { label: string; val: number; color: string; isDerived: boolean }[] = [];
              for (let i = 0; i < slots.length; i++) {
                const slot = slots[i];
                const ownCh = c.channels[slot.cfgIdx];
                if (ownCh && !ownCh.show) continue;
                const divVal = u.data[i + 1]?.[idx];
                if (divVal == null || isNaN(divVal)) continue;
                const eff = getEffectiveChannel(c, slot.cfgIdx);
                const realVal = c.sharedY ? divVal : divVal * eff.vPerDiv + eff.position;
                channels.push({
                  label: slot.label,
                  val: realVal,
                  color: slot.isDerived
                    ? DERIVED_COLORS[slot.colorIdx % DERIVED_COLORS.length]
                    : CHANNEL_COLORS[slot.colorIdx % CHANNEL_COLORS.length],
                  isDerived: slot.isDerived,
                });
              }
              setCursorReadout({
                leftPx: canvasLeft,
                topPx: canvasTop,
                xSec,
                yDiv: yPixelVal,
                yVal,
                yUnit: c.yUnit ?? '',
                channels,
              });
            },
          ],
          setSelect: [
            (u: uPlot) => {
              const { left, width, show } = u.select;
              if (!show || width == null || width <= 0) {
                setSelectedRange(null);
                return;
              }
              const s1 = u.posToVal(left, 'x');
              const s2 = u.posToVal(left + width, 'x');
              setSelectedRange({
                startSec: Math.min(s1, s2),
                endSec: Math.max(s1, s2),
              });
            },
          ],
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
  }, [seriesSignature, themeId]);
}

// ---- 滚轮缩放 ----

export function useWheelZoom(
  containerRef: React.RefObject<HTMLDivElement | null>,
  axisConfigRef: React.MutableRefObject<ScopeAxisConfig>,
  onConfigChange: ((next: ScopeAxisConfig) => void) | undefined,
) {
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const cfgRef = axisConfigRef;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const cfg = cfgRef.current;
      if (e.shiftKey) {
        const firstVisibleIdx = cfg.channels.findIndex((c) => c.show);
        const idx = firstVisibleIdx >= 0 ? firstVisibleIdx : 0;
        const ch = cfg.channels[idx];
        if (!ch) return;
        let vIdx = V_PER_DIV.indexOf(ch.vPerDiv);
        if (vIdx < 0) {
          let bestDiff = Infinity;
          for (let i = 0; i < V_PER_DIV.length; i++) {
            const d = Math.abs(V_PER_DIV[i] - ch.vPerDiv);
            if (d < bestDiff) { bestDiff = d; vIdx = i; }
          }
        }
        const nextV = Math.max(0, Math.min(V_PER_DIV.length - 1, vIdx + (e.deltaY > 0 ? 1 : -1)));
        if (nextV === vIdx) return;
        const newChannels = cfg.channels.slice();
        newChannels[idx] = { ...newChannels[idx], vPerDiv: V_PER_DIV[nextV] };
        onConfigChange?.({ ...cfg, channels: newChannels });
      } else {
        let tbIdx = TIME_BASES_SEC.indexOf(cfg.timeBase);
        if (tbIdx < 0) {
          let bestDiff = Infinity;
          for (let i = 0; i < TIME_BASES_SEC.length; i++) {
            const d = Math.abs(TIME_BASES_SEC[i] - cfg.timeBase);
            if (d < bestDiff) { bestDiff = d; tbIdx = i; }
          }
        }
        const nextTb = Math.max(0, Math.min(TIME_BASES_SEC.length - 1, tbIdx + (e.deltaY > 0 ? 1 : -1)));
        if (nextTb === tbIdx) return;
        onConfigChange?.({ ...cfg, timeBase: TIME_BASES_SEC[nextTb] });
      }
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, [onConfigChange]);
}

// ---- 中键拖拽平移 ----

export function usePanDrag(
  containerRef: React.RefObject<HTMLDivElement | null>,
  plotRef: React.MutableRefObject<uPlot | null>,
  axisConfigRef: React.MutableRefObject<ScopeAxisConfig>,
  onConfigChange: ((next: ScopeAxisConfig) => void) | undefined,
) {
  const panRef = useRef<{ startX: number; startY: number; startHPos: number; startPos: number; chIdx: number } | null>(null);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const onMouseDown = (e: MouseEvent) => {
      if (e.button !== 1) return;
      e.preventDefault();
      const cfg = axisConfigRef.current;
      const firstVisible = cfg.channels.findIndex((c) => c.show);
      const chIdx = firstVisible >= 0 ? firstVisible : 0;
      panRef.current = {
        startX: e.clientX,
        startY: e.clientY,
        startHPos: cfg.hPosition,
        startPos: getEffectiveChannel(cfg, chIdx).position,
        chIdx,
      };
      el.style.cursor = 'grabbing';
    };
    const onMouseMove = (e: MouseEvent) => {
      const ps = panRef.current;
      if (!ps) return;
      const plot = plotRef.current;
      if (!plot) return;
      e.preventDefault();
      const cfg = axisConfigRef.current;
      const dx = e.clientX - ps.startX;
      const dy = e.clientY - ps.startY;
      const winSec = timeBaseToWindowSec(cfg.timeBase);
      const secPerPx = winSec / plot.width;
      const newHPos = ps.startHPos + dx * secPerPx;
      const effCh = getEffectiveChannel(cfg, ps.chIdx);
      const valPerPx = (effCh.vPerDiv * VERTICAL_DIVS) / plot.height;
      const newPos = ps.startPos + dy * valPerPx;
      const newChannels = cfg.channels.slice();
      if (cfg.sharedY) {
        newChannels[0] = { ...newChannels[0], position: newPos };
      } else {
        newChannels[ps.chIdx] = { ...newChannels[ps.chIdx], position: newPos };
      }
      onConfigChange?.({ ...cfg, running: false, hPosition: Math.max(0, newHPos), channels: newChannels });
    };
    const onMouseUp = () => {
      if (panRef.current) {
        panRef.current = null;
        el.style.cursor = '';
      }
    };
    el.addEventListener('mousedown', onMouseDown);
    window.addEventListener('mousemove', onMouseMove);
    window.addEventListener('mouseup', onMouseUp);
    return () => {
      el.removeEventListener('mousedown', onMouseDown);
      window.removeEventListener('mousemove', onMouseMove);
      window.removeEventListener('mouseup', onMouseUp);
    };
  }, [onConfigChange]);
}

// ---- Ctrl/Cmd 隐藏游标 ----

export function useCursorHide() {
  const [cursorHidden, setCursorHidden] = useState(false);
  const isMac = useMemo(
    () => typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform),
    []
  );

  useEffect(() => {
    const isInputTarget = (t: EventTarget | null) => {
      if (!(t instanceof HTMLElement)) return false;
      const tag = t.tagName.toLowerCase();
      return tag === 'input' || tag === 'textarea' || tag === 'select' || t.isContentEditable;
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (isInputTarget(e.target)) return;
      if (e.ctrlKey || e.metaKey) setCursorHidden(true);
    };
    const onKeyUp = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey)) setCursorHidden(false);
    };
    window.addEventListener('keydown', onKeyDown);
    window.addEventListener('keyup', onKeyUp);
    return () => {
      window.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('keyup', onKeyUp);
    };
  }, []);

  return { cursorHidden, isMac };
}

// ---- Tooltip 定位 ----

export function useTooltipPos(
  cursorReadout: { leftPx: number; topPx: number } | null,
  containerRef: React.RefObject<HTMLDivElement | null>,
  tooltipRef: React.RefObject<HTMLDivElement | null>,
) {
  const [tooltipPos, setTooltipPos] = useState<{ left: number; top: number } | null>(null);

  useLayoutEffect(() => {
    if (!cursorReadout) {
      setTooltipPos(null);
      return;
    }
    const tooltip = tooltipRef.current;
    const container = containerRef.current;
    if (!tooltip || !container) return;
    const tw = tooltip.offsetWidth;
    const th = tooltip.offsetHeight;
    const cw = container.clientWidth;
    const ch = container.clientHeight;
    const gap = 12;
    let left = cursorReadout.leftPx + gap;
    let top = cursorReadout.topPx + gap;
    if (left + tw > cw) {
      left = cursorReadout.leftPx - gap - tw;
    }
    if (top + th > ch) {
      top = cursorReadout.topPx - gap - th;
    }
    left = Math.max(0, left);
    top = Math.max(0, top);
    setTooltipPos({ left, top });
  }, [cursorReadout, containerRef, tooltipRef]);

  return tooltipPos;
}
