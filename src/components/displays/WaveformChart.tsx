import { useRef, useEffect, useLayoutEffect, useCallback, useMemo, useState } from 'react';
import uPlot from 'uplot';
import 'uplot/dist/uPlot.min.css';
import { useAppStore } from '../../store/appStore';
import { waveformWindow } from '../../lib/dataBuffer';
import { t } from '../../i18n';
import type { WidgetConfig } from '../../types';
import { TIME_BASES_SEC, V_PER_DIV, formatVPerDiv, getEffectiveChannel, type ScopeAxisConfig } from '../../types';
import { timeBaseToWindowSec, VERTICAL_DIVS } from '../../lib/scopeUtils';
import {
  CHANNEL_COLORS, DERIVED_COLORS, TEXT_COLOR, GRID_COLOR, TICK_COLOR, CURSOR_COLOR, getContainerSize,
} from './waveformConstants';
import { WaveformTimeline } from './WaveformTimeline';

interface WaveformChartProps {
  widget: Extract<WidgetConfig, { kind: 'Waveform' }>;
  axisConfig: ScopeAxisConfig;
  onConfigChange?: (next: ScopeAxisConfig) => void;
}

/// 波形图连接的输入 — 可以是原始通道或派生节点 (Math/Filter 等)
type ConnectedInput =
  | { kind: 'channel'; idx: number }
  | { kind: 'derived'; sourceId: string; sourceHandle: string };

/// 系列 slot — 用于 series 创建/数据获取/游标读数
/// channelIdx: 通道索引 (用于颜色和 effective channel 配置)
/// derivedIdx: 派生 series 索引 (用于颜色, -1 表示非派生)
interface SeriesSlot {
  input: ConnectedInput;
  /// 通道颜色索引 (channel 用 idx, derived 用 derivedIdx)
  colorIdx: number;
  /// 是否为派生 series
  isDerived: boolean;
  /// 显示标签 (CH0 / MATH:widgetId)
  label: string;
  /// 用于 effective channel 配置查询的索引
  /// channel: 用 idx; derived: 用 widget.params.channels + derivedIdx
  cfgIdx: number;
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
  const frozenDataRef = useRef<{
    ts: number[];
    chs: number[][];
    derived?: Record<string, Record<string, number[]>>;
  } | null>(null);

  // 鼠标悬停读数 — { leftPx, topPx, xSec, yDiv, channelValues: [{label, val, color, isDerived}] }
  const [cursorReadout, setCursorReadout] = useState<{
    leftPx: number;
    topPx: number;
    xSec: number;
    yDiv: number;
    yVal: number;
    yUnit: string;
    channels: { label: string; val: number; color: string; isDerived: boolean }[];
  } | null>(null);

  // tooltip 实际渲染位置 (经边界检测后的像素坐标, 相对 waveform-container)
  // 默认放在十字线右下角, 超出容器边界时翻转到左侧/上方
  const tooltipRef = useRef<HTMLDivElement>(null);
  const [tooltipPos, setTooltipPos] = useState<{ left: number; top: number } | null>(null);

  // Ctrl (Win/Linux) / Cmd (Mac) 按下时隐藏光标十字线 + 读数, 方便观察波形细节
  const [cursorHidden, setCursorHidden] = useState(false);
  const isMac = useMemo(
    () => typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform),
    []
  );

  const lang = useAppStore((s) => s.lang);
  const rfEdges = useAppStore((s) => s.rfEdges);
  const updateWidget = useAppStore((s) => s.updateWidget);

  const viewEndSec = axisConfig.running ? 0 : -axisConfig.hPosition;
  const timeWindowSec = timeBaseToWindowSec(axisConfig.timeBase);

  // 解析连接的输入 — 通道 + 派生 (Math/Filter 等)
  const connectedInputs = useMemo<ConnectedInput[]>(() => {
    if (widget.params.id === 'default-waveform') {
      const win = waveformWindow.get();
      const count = win.channel_count || widget.params.channels;
      return Array.from({ length: count }, (_, i) => ({ kind: 'channel' as const, idx: i }));
    }
    const channels: ConnectedInput[] = [];
    const derived: ConnectedInput[] = [];
    for (const e of rfEdges) {
      if (e.target !== widget.params.id) continue;
      const handle = e.sourceHandle ?? '';
      const m = /^ch(\d+)$/.exec(handle);
      if (m) {
        channels.push({ kind: 'channel', idx: parseInt(m[1], 10) });
      } else {
        derived.push({ kind: 'derived', sourceId: e.source, sourceHandle: handle });
      }
    }
    // 通道在前, 派生在后
    return [...channels, ...derived];
  }, [rfEdges, widget.params.id, widget.params.channels]);

  // 派生的通道索引列表 (供 WaveformTimeline 缩略图使用)
  const connectedChannels = useMemo(
    () => connectedInputs
      .filter((i): i is Extract<ConnectedInput, { kind: 'channel' }> => i.kind === 'channel')
      .map((i) => i.idx),
    [connectedInputs]
  );

  // 构建 series slots — 决定实际显示的 series 数量和顺序
  // dynamicSeries=false (默认): widget.params.channels 通道槽 + 派生槽 (未连接通道填 NaN)
  // dynamicSeries=true: 仅连接的通道 + 派生 (无未连接槽)
  const seriesSlots = useMemo<SeriesSlot[]>(() => {
    const isDynamic = widget.params.dynamicSeries ?? false;
    const channelInputs = connectedInputs.filter(
      (i): i is Extract<ConnectedInput, { kind: 'channel' }> => i.kind === 'channel'
    );
    const derivedInputs = connectedInputs.filter(
      (i): i is Extract<ConnectedInput, { kind: 'derived' }> => i.kind === 'derived'
    );
    const slots: SeriesSlot[] = [];

    if (isDynamic) {
      // 动态: 仅连接的通道 + 派生
      for (const input of channelInputs) {
        slots.push({
          input,
          colorIdx: input.idx,
          isDerived: false,
          label: `CH${input.idx}`,
          cfgIdx: input.idx,
        });
      }
      for (let i = 0; i < derivedInputs.length; i++) {
        const input = derivedInputs[i];
        slots.push({
          input,
          colorIdx: i,
          isDerived: true,
          label: `MATH:${input.sourceId}`,
          cfgIdx: widget.params.channels + i,
        });
      }
    } else {
      // 固定: widget.params.channels 通道槽 + 派生槽
      for (let i = 0; i < widget.params.channels; i++) {
        const input = channelInputs.find((x) => x.idx === i);
        if (input) {
          slots.push({
            input,
            colorIdx: i,
            isDerived: false,
            label: `CH${i}`,
            cfgIdx: i,
          });
        } else {
          // 未连接的占位槽 (data 将填 NaN)
          slots.push({
            input: { kind: 'channel', idx: i },
            colorIdx: i,
            isDerived: false,
            label: `CH${i}`,
            cfgIdx: i,
          });
        }
      }
      for (let i = 0; i < derivedInputs.length; i++) {
        const input = derivedInputs[i];
        slots.push({
          input,
          colorIdx: i,
          isDerived: true,
          label: `MATH:${input.sourceId}`,
          cfgIdx: widget.params.channels + i,
        });
      }
    }
    return slots;
  }, [connectedInputs, widget.params.channels, widget.params.dynamicSeries]);

  const isConnected = widget.params.id === 'default-waveform' || connectedInputs.length > 0;

  // series signature — 用于 useLayoutEffect 依赖, 在 slots 数量/标签变化时重建 uPlot
  const seriesSignature = useMemo(
    () => seriesSlots.map((s) => `${s.isDerived ? 'd' : 'c'}${s.label}`).join(','),
    [seriesSlots]
  );

  // 缓存 seriesSlots 供 getDisplayData (useCallback) 使用, 避免 effect 重建
  const seriesSlotsRef = useRef(seriesSlots);
  useEffect(() => { seriesSlotsRef.current = seriesSlots; }, [seriesSlots]);

  // 监听 Ctrl/Cmd 按键 — 按下时隐藏光标十字线与读数, 方便观察波形
  // 忽略输入框中的按键 (避免影响表单编辑)
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

  // 计算 tooltip 位置 — 紧贴十字线右下角, 超出容器边界时翻转到左侧/上方
  // 用 useLayoutEffect 在 DOM 渲染后测量 tooltip 实际尺寸, 避免预估不准
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
    const gap = 12; // 十字线与 tooltip 的间距
    let left = cursorReadout.leftPx + gap;
    let top = cursorReadout.topPx + gap;
    // 右边界超出 → 翻转到十字线左侧
    if (left + tw > cw) {
      left = cursorReadout.leftPx - gap - tw;
    }
    // 下边界超出 → 翻转到十字线上方
    if (top + th > ch) {
      top = cursorReadout.topPx - gap - th;
    }
    // 防止负值
    left = Math.max(0, left);
    top = Math.max(0, top);
    setTooltipPos({ left, top });
  }, [cursorReadout]);

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
    // 为每个 slot 构建 data array
    const seriesArrays = slots.map((slot) => {
      let arr: number[] | undefined;
      if (slot.input.kind === 'channel') {
        arr = channelArrays[slot.input.idx];
      } else {
        arr = derivedMap?.[widget.params.id]?.[slot.input.sourceId];
      }
      if (!arr) return Array(tsLen).fill(NaN);
      if (arr.length === tsLen) return arr;
      // 对齐: 截断或补 NaN
      const padded = arr.slice(0, tsLen);
      while (padded.length < tsLen) padded.push(NaN);
      return padded;
    });

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

  // 配置变化 → 更新 ref + 通道可见性 + 重新归一化数据
  // 关键: V/div 或 position 变化时, 必须重新 setData, 否则波形不会按新档位重绘
  useEffect(() => {
    axisConfigRef.current = axisConfig;
    const plot = plotRef.current;
    if (!plot) return;
    const slots = seriesSlotsRef.current;
    for (let i = 0; i < slots.length; i++) {
      plot.setSeries(i + 1, { show: axisConfig.channels[slots[i].cfgIdx]?.show ?? true });
    }
    // 重新归一化数据 (用新的 vPerDiv / position 重新计算 div 值)
    plot.setData(getDisplayData() as unknown as uPlot.AlignedData);
    plot.redraw();
  }, [axisConfig, seriesSlots, getDisplayData]);

  // 冻结快照 — Stop 时拍下当前数据 (含 derived), Run 时清空
  // 关键: 拍快照后必须立即重绘, 因为 axisConfig effect 可能先于本 effect 执行
  // (此时 frozenDataRef 还是 null, 导致 setData 空数据使图变黑)
  useEffect(() => {
    if (!axisConfig.running) {
      const win = waveformWindow.get();
      if (win.timestamps.length > 0 && !frozenDataRef.current) {
        frozenDataRef.current = {
          ts: [...win.timestamps],
          chs: win.channels.map((ch) => [...ch]),
          derived: win.derived
            ? Object.fromEntries(
                Object.entries(win.derived).map(([k1, v1]) => [
                  k1,
                  Object.fromEntries(
                    Object.entries(v1).map(([k2, v2]) => [k2, [...v2]])
                  ),
                ])
              )
            : undefined,
        };
        // 拍快照后立即用冻结数据重绘
        const plot = plotRef.current;
        if (plot) {
          plot.setData(getDisplayData() as unknown as uPlot.AlignedData);
          plot.redraw();
        }
      }
    } else {
      frozenDataRef.current = null;
    }
  }, [axisConfig.running, getDisplayData]);

  // 初始化 uPlot
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
      const series: uPlot.Series[] = [{
        label: 't', stroke: TEXT_COLOR,
        value: (_u, v) => (v == null ? '--' : formatTimeMs(v * 1000)),
      }];
      for (const slot of slots) {
        const chCfg = getEffectiveChannel(cfg0, slot.cfgIdx);
        const vPerDiv = chCfg.vPerDiv;
        const pos = chCfg.position;
        // 通道可见性始终 per-slot (不共用)
        const show = cfg0.channels[slot.cfgIdx]?.show ?? true;
        const color = slot.isDerived
          ? DERIVED_COLORS[slot.colorIdx % DERIVED_COLORS.length]
          : CHANNEL_COLORS[slot.colorIdx % CHANNEL_COLORS.length];
        series.push({
          label: `${slot.label} ${formatVPerDiv(vPerDiv, yUnit)}`,
          stroke: color,
          width: 1.5,
          // value 用于 legend/tooltip: sharedY 时数据已是真实值, 直接显示; 否则反归一化
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
      const gridStroke = cfg.grid ? GRID_COLOR : 'transparent';
      return {
        width: w, height: h, series: createSeries(),
        axes: [
          {
            stroke: TEXT_COLOR, grid: { stroke: gridStroke, width: 1 },
            ticks: { stroke: TICK_COLOR }, size: 32, gap: 4,
            // label 留空: 单位由 values 函数动态附加到每个刻度 (因为 label 是静态字符串无法同步)
            label: '',
            labelSize: 20, labelFont: '11px sans-serif',
            // values 是函数, 每次 redraw 调用, 根据当前时基窗口自适应单位 (s/ms/µs)
            values: (_self, ticks) => {
              const c = axisConfigRef.current;
              const winSec = timeBaseToWindowSec(c.timeBase);
              if (winSec >= 1) {
                // 窗口 >= 1s → 显示 s
                return ticks.map((v) => v.toFixed(2) + 's');
              }
              if (winSec >= 0.001) {
                // 窗口 >= 1ms → 显示 ms
                return ticks.map((v) => (v * 1000).toFixed(1) + 'ms');
              }
              // 窗口 < 1ms → 显示 µs
              return ticks.map((v) => (v * 1e6).toFixed(0) + 'µs');
            },
          },
          {
            stroke: TEXT_COLOR, grid: { stroke: gridStroke, width: 1 },
            ticks: { stroke: TICK_COLOR }, size: 50, gap: 4,
            // label 在 sharedY 切换时无法动态更新 (uPlot 限制), 用空字符串避免误导
            label: '',
            labelSize: 16, labelFont: '11px sans-serif',
            // values 是函数, 每次 redraw 重新调用, 可动态根据 sharedY 切换显示
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
          // sharedY: 真实值范围 [pos - vPerDiv*4, pos + vPerDiv*4]
          // 独立 Y: div 范围 [-4, 4]
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
              const yPixelVal = u.posToVal(top, 'y'); // sharedY=真实值, 独立Y=div
              // u.cursor.left/top 是相对绘图区域 (不含 Y 轴/X 轴 padding)
              // tooltip 定位是相对整个 canvas (= waveform-container)
              // 需加上绘图区域偏移: u.bbox.left/top
              const bbox = (u as unknown as { bbox?: { left: number; top: number } }).bbox;
              const plotLeft = bbox?.left ?? 0;
              const plotTop = bbox?.top ?? 0;
              const canvasLeft = left + plotLeft;
              const canvasTop = top + plotTop;
              // 用第一可见 slot 反归一化 Y 像素位置 (独立 Y 时)
              const slots = seriesSlotsRef.current;
              const firstVisibleIdx = slots.findIndex((s) => c.channels[s.cfgIdx]?.show ?? true);
              const slotIdx = firstVisibleIdx >= 0 ? firstVisibleIdx : 0;
              const firstSlot = slots[slotIdx];
              const chCfg = firstSlot
                ? getEffectiveChannel(c, firstSlot.cfgIdx)
                : { vPerDiv: 1, position: 0 };
              const yVal = c.sharedY ? yPixelVal : yPixelVal * chCfg.vPerDiv + chCfg.position;
              // 收集所有可见 slot 在 idx 处的值 (反归一化为实际值)
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
  }, [seriesSignature]);

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
  // Shift+滚轮 → 第一可见通道 V/div 档位 (Y 轴缩放)
  // 使用原生非被动监听器确保 preventDefault 生效 (React onWheel 默认 passive)
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const cfgRef = axisConfigRef;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const cfg = cfgRef.current;
      if (e.shiftKey) {
        // Y 轴缩放: 调整第一个可见通道的 V/div
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
        // X 轴缩放: 调整时基
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
    // passive: false 才能调用 preventDefault
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, [onConfigChange]);

  // 鼠标中键拖动 → 平移图形 (X: hPosition, Y: 第一可见通道 position)
  // 中键 mousedown 在 container 上触发, mousemove/mouseup 绑定 window 避免拖出丢失
  const panRef = useRef<{ startX: number; startY: number; startHPos: number; startPos: number; chIdx: number } | null>(null);
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const onMouseDown = (e: MouseEvent) => {
      if (e.button !== 1) return; // 仅中键
      e.preventDefault(); // 阻止浏览器自动滚动
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
      // X: 像素 → 秒 (hPosition >= 0, 0=实时, 正数=查看历史)
      // 向右拖图形跟随右移 → 看更早数据 → hPosition 增大
      const winSec = timeBaseToWindowSec(cfg.timeBase);
      const secPerPx = winSec / plot.width;
      const newHPos = ps.startHPos + dx * secPerPx;
      // Y: 像素 → 值 (向下拖 = position 增加 = 波形下移, 图形跟随鼠标)
      const effCh = getEffectiveChannel(cfg, ps.chIdx);
      const valPerPx = (effCh.vPerDiv * VERTICAL_DIVS) / plot.height;
      const newPos = ps.startPos + dy * valPerPx;
      // 应用新配置 (running 时 hPosition 不生效, 切到 Stop 让平移可见)
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
    // mousedown 在 container, move/up 在 window (拖出不丢失)
    el.addEventListener('mousedown', onMouseDown);
    window.addEventListener('mousemove', onMouseMove);
    window.addEventListener('mouseup', onMouseUp);
    return () => {
      el.removeEventListener('mousedown', onMouseDown);
      window.removeEventListener('mousemove', onMouseMove);
      window.removeEventListener('mouseup', onMouseUp);
    };
  }, [onConfigChange]);

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
    const chCfg = getEffectiveChannel(axisConfig, chIdx);
    const vPerDiv = chCfg.vPerDiv;
    const pos = chCfg.position;
    let c1R: number, c2R: number;
    if (axisConfig.sharedY) {
      // 共用 Y: Y 轴是真实值, range = [pos - vPerDiv*4, pos + vPerDiv*4]
      const yMin = pos - (vPerDiv * VERTICAL_DIVS) / 2;
      const yMax = pos + (vPerDiv * VERTICAL_DIVS) / 2;
      const yRange = yMax - yMin || 1;
      c1R = 1 - (cfg.c1 - yMin) / yRange;
      c2R = 1 - (cfg.c2 - yMin) / yRange;
    } else {
      // 独立 Y: Y 轴是 div, range = [-4, 4]
      const c1Div = (cfg.c1 - pos) / vPerDiv;
      const c2Div = (cfg.c2 - pos) / vPerDiv;
      c1R = 1 - (c1Div + VERTICAL_DIVS / 2) / VERTICAL_DIVS;
      c2R = 1 - (c2Div + VERTICAL_DIVS / 2) / VERTICAL_DIVS;
    }
    return (
      <>
        <line x1="0" y1={`${c1R * 100}%`} x2="100%" y2={`${c1R * 100}%`} stroke={CURSOR_COLOR} strokeWidth={1} strokeDasharray="4 2" />
        <line x1="0" y1={`${c2R * 100}%`} x2="100%" y2={`${c2R * 100}%`} stroke={CURSOR_COLOR} strokeWidth={1} strokeDasharray="4 2" />
      </>
    );
  }, [axisConfig.cursors, axisConfig.channels, axisConfig.sharedY, axisConfig.running, axisConfig.hPosition, timeWindowSec, connectedChannels]);

  return (
    <div className="waveform-layout" style={{ flexDirection: 'column' }}>
      <div
        className={`waveform-container ${cursorHidden ? 'cursor-hidden' : ''}`}
        ref={containerRef}
        onMouseLeave={() => setCursorReadout(null)}
        style={{ flex: 1, minHeight: 0, position: 'relative' }}
      >
        {axisConfig.cursors.enabled && (
          <svg className="cursor-overlay" style={{ position: 'absolute', inset: 0, pointerEvents: 'none', zIndex: 5 }}>
            {cursorOverlay}
          </svg>
        )}

        {/* 左上角提示: 按住 Ctrl/Cmd 隐藏光标 */}
        <div className="cursor-hint">
          {cursorHidden
            ? t(lang, 'cursorHiddenHint')
            : (isMac ? '⌘ ' : 'Ctrl ') + t(lang, 'cursorHideHint')}
        </div>

        {/* 右下角动态 series 开关 (仅用户创建的波形图, default-waveform 不显示) */}
        {widget.params.id !== 'default-waveform' && (
          <button
            className={`dynamic-series-toggle ${widget.params.dynamicSeries ? 'active' : ''}`}
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

        {/* 鼠标悬停读数: 合并到单个 tooltip 放在十字线右下角 (边界翻转), 避免遮住中间波形 */}
        {cursorReadout && !cursorHidden && (
          <div
            ref={tooltipRef}
            className="cursor-readout-tooltip"
            style={{
              position: 'absolute',
              // tooltipPos 由 useLayoutEffect 测量后设置; 首帧用默认位置 (会被同步覆盖, 不闪烁)
              left: tooltipPos ? tooltipPos.left : cursorReadout.leftPx + 12,
              top: tooltipPos ? tooltipPos.top : cursorReadout.topPx + 12,
              pointerEvents: 'none',
              zIndex: 9,
            }}
          >
            <div className="cursor-readout-row">
              <span className="cursor-readout-label">X</span>
              <span className="cursor-readout-val">
                {formatTimeMs(cursorReadout.xSec * 1000)}
              </span>
            </div>
            <div className="cursor-readout-row">
              <span className="cursor-readout-label">Y</span>
              <span className="cursor-readout-val">
                {formatYValue(cursorReadout.yVal, cursorReadout.yUnit)}
              </span>
            </div>
            {cursorReadout.channels.length > 1 && (
              <div className="cursor-readout-divider" />
            )}
            {cursorReadout.channels.map((ch, i) => (
              <div key={ch.label + i} className="cursor-readout-channel">
                <span
                  className="cursor-readout-dot"
                  style={{ background: ch.color }}
                />
                <span className="cursor-readout-ch-label">{ch.label}</span>
                <span className="cursor-readout-ch-val">
                  {formatYValue(ch.val, cursorReadout.yUnit)}
                </span>
              </div>
            ))}
          </div>
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
        frozenData={!axisConfig.running ? frozenDataRef.current : null}
        onConfigChange={onConfigChange}
      />
    </div>
  );
}

/// 格式化时间 (毫秒) — 示波器风格: 自动选择 µs/ms/s 单位, 负数表示过去时间
/// 例: -250 → "-250ms", 1.5 → "1.500ms", 1500 → "1.500s", 0.1 → "100µs"
function formatTimeMs(ms: number): string {
  const sign = ms < 0 ? '-' : '';
  const abs = Math.abs(ms);
  if (abs >= 1000) return sign + (abs / 1000).toFixed(3) + 's';
  if (abs >= 1) return sign + abs.toFixed(abs < 10 ? 3 : abs < 100 ? 2 : 1) + 'ms';
  return sign + (abs * 1000).toFixed(0) + 'µs';
}

/// 格式化 Y 轴值 — 不使用 µ/m/k 前缀, 大/小值用科学计数法, 中间值用普通小数
/// unit 为空字符串时不附加单位 (示波器默认配置)
/// 例: (1.234, 'V') → "1.234V", (0.0001234, 'A') → "1.23e-4A", (12345, '') → "1.23e+4"
function formatYValue(val: number, unit: string): string {
  const u = unit || '';
  const abs = Math.abs(val);
  if (abs === 0) return '0' + u;
  // 大值 (>=1e4) 或小值 (<1e-3) 用科学计数法
  if (abs >= 1e4 || abs < 1e-3) return val.toExponential(2) + u;
  // 中间值用普通小数, 自适应位数
  if (abs >= 100) return val.toFixed(2) + u;
  if (abs >= 1) return val.toFixed(3) + u;
  return val.toFixed(4) + u;
}
