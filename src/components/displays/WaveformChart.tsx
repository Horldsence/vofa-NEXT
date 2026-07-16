import { useRef, useEffect, useLayoutEffect } from 'react';
import UPlot from 'uplot';
import 'uplot/dist/uPlot.min.css';
import type { WidgetConfig } from '../../types';
import { waveformBuffer } from '../../lib/dataBuffer';
import type { WaveformAxisConfig } from './AxisSettings';

interface WaveformChartProps {
  widget: Extract<WidgetConfig, { kind: 'Waveform' }>;
  axisConfig: WaveformAxisConfig;
}

/// 坐标轴颜色 — 使用较高亮度确保暗色主题可见
const TEXT_COLOR = '#bbbbbb';
const GRID_COLOR = '#444444';
const TICK_COLOR = '#555555';

const CHANNEL_COLORS = [
  '#75beff', '#89d185', '#e2c08d', '#f48771',
  '#c586c0', '#4ec9b0', '#dcdcaa', '#9cdcfe',
];

/// 获取容器的有效尺寸, 未布局完成时使用兜底值
function getContainerSize(container: HTMLElement) {
  const rect = container.getBoundingClientRect();
  return {
    w: Math.max(Math.floor(rect.width), 300),
    h: Math.max(Math.floor(rect.height), 200),
  };
}

/// 波形图控件 — 基于 uPlot 的高性能波形显示
export function WaveformChart({ widget, axisConfig }: WaveformChartProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const plotRef = useRef<UPlot | null>(null);
  const axisConfigRef = useRef(axisConfig);
  const lastVersionRef = useRef(-1);

  // 配置变化时更新通道可见性
  useEffect(() => {
    axisConfigRef.current = axisConfig;
    const plot = plotRef.current;
    if (!plot) return;
    for (let i = 0; i < widget.params.channels; i++) {
      const visible = axisConfig.visibleChannels[i] ?? true;
      plot.setSeries(i + 1, { show: visible });
    }
    plot.redraw();
  }, [axisConfig, widget.params.channels]);

  // 初始化 uPlot
  useLayoutEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    let plot: UPlot | null = null;
    let resizeRaf: number | null = null;
    let lastW = 0;
    let lastH = 0;

    const createSeries = (): UPlot.Series[] => {
      const channels = widget.params.channels;
      const series: UPlot.Series[] = [
        {
          label: 't',
          stroke: TEXT_COLOR,
          value: (_u, v) => (v == null ? '--' : v.toFixed(3) + 's'),
        },
      ];
      for (let i = 0; i < channels; i++) {
        series.push({
          label: `CH${i}`,
          stroke: CHANNEL_COLORS[i % CHANNEL_COLORS.length],
          width: 1.5,
          value: (_u, v) => (v == null ? '--' : v.toFixed(2)),
          show: axisConfig.visibleChannels[i] ?? true,
        });
      }
      return series;
    };

    const createOptions = (w: number, h: number): UPlot.Options => {
      const cfg = axisConfigRef.current;
      const gridStroke = cfg.grid ? GRID_COLOR : 'transparent';

      return {
        width: w,
        height: h,
        series: createSeries(),
        axes: [
          {
            // X 轴 (底部)
            stroke: TEXT_COLOR,
            grid: { stroke: gridStroke, width: 1 },
            ticks: { stroke: TICK_COLOR },
            size: 24,
            gap: 4,
            values: (_self, ticks) => ticks.map((v) => v.toFixed(1)),
          },
          {
            // Y 轴 (左侧)
            stroke: TEXT_COLOR,
            grid: { stroke: gridStroke, width: 1 },
            ticks: { stroke: TICK_COLOR },
            size: 40,
            gap: 4,
            values: (_self, ticks) => ticks.map((v) => v.toFixed(1)),
          },
        ],
        // 关闭默认 legend, 避免占用底部空间导致 X 轴被挤出
        legend: { show: false },
        cursor: { points: { size: 4 } },
        scales: {
          x: {
            time: false,
            range: (_self, dataMin, dataMax) => {
              const c = axisConfigRef.current.x;
              if (dataMin == null || dataMax == null || dataMin === dataMax) {
                return c.auto ? [0, 10] : [c.min, c.max];
              }
              return c.auto ? [dataMin, dataMax] : [c.min, c.max];
            },
          },
          y: {
            range: (_self, dataMin, dataMax) => {
              const c = axisConfigRef.current.y;
              if (dataMin == null || dataMax == null || dataMin === dataMax) {
                return c.auto ? [-1, 1] : [c.min, c.max];
              }
              return c.auto ? [dataMin, dataMax] : [c.min, c.max];
            },
          },
        },
      };
    };

    // 初始使用一个虚拟点, 确保空数据时坐标轴刻度可见
    const placeholderData = (): number[][] => {
      const channels = widget.params.channels;
      return [[0], ...Array.from({ length: channels }, () => [NaN])];
    };

    const createPlot = () => {
      const { w, h } = getContainerSize(container);
      plot = new UPlot(
        createOptions(w, h),
        placeholderData() as unknown as UPlot.AlignedData,
        container
      );
      plotRef.current = plot;
      lastW = w;
      lastH = h;
    };

    const resize = () => {
      const { w, h } = getContainerSize(container);
      if (w === lastW && h === lastH) return;
      if (!plot) {
        createPlot();
        return;
      }
      if (resizeRaf !== null) cancelAnimationFrame(resizeRaf);
      resizeRaf = requestAnimationFrame(() => {
        plot?.setSize({ width: w, height: h });
        lastW = w;
        lastH = h;
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

  // 数据更新
  useEffect(() => {
    let rafId: number | null = null;
    const tick = () => {
      if (plotRef.current) {
        const version = waveformBuffer.version;
        if (version !== lastVersionRef.current) {
          lastVersionRef.current = version;
          const data = waveformBuffer.getData();
          plotRef.current.setData(data as unknown as UPlot.AlignedData);
        }
      }
      rafId = requestAnimationFrame(tick);
    };
    rafId = requestAnimationFrame(tick);
    return () => {
      if (rafId !== null) cancelAnimationFrame(rafId);
    };
  }, []);

  return <div className="waveform-container" ref={containerRef} />;
}
