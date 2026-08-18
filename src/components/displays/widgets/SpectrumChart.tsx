import { memo, useEffect, useRef, useState } from 'react';
import { Settings2 } from 'lucide-react';
import type { WidgetConfig, SpectrumOutput } from '../../../types';
import { useAppStore } from '../../../store/appStore';
import { t } from '../../../i18n';
import { getThemeColor } from '../waveform/wavechartFormatters';

interface SpectrumChartProps {
  widget: Extract<WidgetConfig, { kind: 'Spectrum' }>;
  onRemove: () => void;
  onEdit?: () => void;
}

/// 频谱展示控件 — 纯展示, 不做任何求解
///
/// 数据流:
///   1. 节点图中把某个 FFT 求解器的 spectrum 输出连到本控件的 spectrum 输入端口
///      (连线即数据源; 旧布局无连线时回退到 params.sourceId 兼容)
///   2. 后端 FFT 求解器把结果推入专用频谱数据通道 (spectrumResults[sourceId])
///   3. 本组件仅从 store.spectrumResults[sourceId] 读取最新结果并绘制
///
/// 与旧实现的区别: 本控件不再拥有 windowSize/windowType/output/sampleRate
/// 等求解参数 — 这些已上移到 FFT 求解器, 本控件只负责展示。
export const SpectrumChart = memo(function SpectrumChart({ widget, onEdit }: SpectrumChartProps) {
  const { id } = widget.params;
  // 所有 FFT 求解器 (数据源候选)
  const widgets = useAppStore((s) => s.widgets);
  const fftWidgets = widgets.filter((w): w is Extract<WidgetConfig, { kind: 'FFT' }> => w.kind === 'FFT');
  const rfEdges = useAppStore((s) => s.rfEdges);
  // 数据源 = 连到本控件 spectrum 输入端口的 FFT 求解器 (节点 id 即 widget id)
  const connectedSourceId =
    rfEdges.find((e) => e.target === id && e.targetHandle === 'spectrum')?.source ?? widget.params.sourceId;
  const sourceWidget = fftWidgets.find((w) => w.params.id === connectedSourceId) ?? null;
  const sourceId = sourceWidget ? connectedSourceId : null;
  // 只订阅本控件数据源 (sourceId) 的频谱结果, 避免全局 spectrumResults 更新时所有 SpectrumChart 重渲染
  const result = useAppStore((s) => (sourceId ? s.spectrumResults[sourceId] : undefined));
  const lang = useAppStore((s) => s.lang);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  // 跟踪容器尺寸, 触发 canvas 重绘 (响应式)
  const [size, setSize] = useState({ w: 0, h: 0 });
  // 十字光标位置 (相对 canvas 的 CSS 像素坐标), null = 隐藏
  const [cursor, setCursor] = useState<{ x: number; y: number } | null>(null);

  // 展示所需的上下文来自数据源 FFT 求解器 (无数据源时用默认值)
  const output: SpectrumOutput = sourceWidget?.params.output ?? 'Magnitude';
  const sampleRate = sourceWidget?.params.sampleRate ?? 1000;
  const windowSize = sourceWidget?.params.windowSize ?? 0;

  // ResizeObserver: 容器尺寸变化时更新 size, 触发重绘
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const parent = canvas.parentElement;
    if (!parent) return;
    const ro = new ResizeObserver((entries) => {
      const r = entries[0].contentRect;
      setSize({ w: Math.floor(r.width), h: Math.floor(r.height) });
    });
    ro.observe(parent);
    return () => ro.disconnect();
  }, []);

  // 绘制频谱图
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    if (size.w === 0 || size.h === 0) return;

    const dpr = window.devicePixelRatio || 1;
    canvas.width = size.w * dpr;
    canvas.height = size.h * dpr;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.scale(dpr, dpr);

    const w = size.w;
    const h = size.h;
    ctx.clearRect(0, 0, w, h);

    // 背景
    ctx.fillStyle = '#1e1e1e';
    ctx.fillRect(0, 0, w, h);

    // 网格 (4x4)
    ctx.strokeStyle = '#333333';
    ctx.lineWidth = 1;
    for (let i = 1; i < 4; i++) {
      const x = (i / 4) * w;
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, h);
      ctx.stroke();
    }
    for (let i = 1; i < 4; i++) {
      const y = (i / 4) * h;
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(w, y);
      ctx.stroke();
    }

    // 未选择数据源: 引导用户选择
    if (!sourceId) {
      ctx.fillStyle = '#888888';
      ctx.font = '11px sans-serif';
      ctx.textAlign = 'center';
      ctx.fillText(t(lang, 'spectrumNoSource'), w / 2, h / 2 - 8);
      ctx.fillStyle = '#666666';
      ctx.fillText(t(lang, 'spectrumNoSourceHint'), w / 2, h / 2 + 10);
      return;
    }

    if (!result || result.values.length === 0) {
      // 无数据提示
      ctx.fillStyle = '#666666';
      ctx.font = '11px sans-serif';
      ctx.textAlign = 'center';
      ctx.fillText(t(lang, 'spectrumWaiting'), w / 2, h / 2);
      return;
    }

    const values = result.values;
    const freqs = result.frequencies;
    const n = values.length;
    const maxFreq = freqs[freqs.length - 1] || sampleRate / 2;

    // 计算 Y 范围 (对数模式下避免 0/负数)
    let vMin = Infinity;
    let vMax = -Infinity;
    for (const v of values) {
      if (Number.isFinite(v)) {
        if (v < vMin) vMin = v;
        if (v > vMax) vMax = v;
      }
    }
    if (!Number.isFinite(vMin) || !Number.isFinite(vMax)) {
      vMin = 0;
      vMax = 1;
    }
    if (vMin === vMax) {
      vMax = vMin + 1;
    }
    // 给 Y 范围加一点边距
    const yRange = vMax - vMin;
    vMax += yRange * 0.05;
    vMin -= yRange * 0.05;

    // 绘制频谱曲线 (橙色, 与 DERIVED_COLORS[0] 一致)
    ctx.strokeStyle = '#ff8c42';
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    for (let i = 0; i < n; i++) {
      const x = (i / (n - 1)) * w;
      const y = h - ((values[i] - vMin) / (vMax - vMin)) * h;
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.stroke();

    // 填充下方区域
    ctx.lineTo(w, h);
    ctx.lineTo(0, h);
    ctx.closePath();
    ctx.fillStyle = 'rgba(255, 140, 66, 0.15)';
    ctx.fill();

    // 频率轴标签 (左/中/右)
    ctx.fillStyle = '#888888';
    ctx.font = '9px sans-serif';
    ctx.textAlign = 'left';
    ctx.fillText('0', 2, h - 2);
    ctx.textAlign = 'center';
    ctx.fillText(formatFreq(maxFreq / 2), w / 2, h - 2);
    ctx.textAlign = 'right';
    ctx.fillText(formatFreq(maxFreq), w - 2, h - 2);

    // Y 轴标签 (max/min)
    ctx.textAlign = 'left';
    ctx.fillStyle = '#aaaaaa';
    ctx.fillText(formatValue(vMax, output), 2, 10);
    ctx.fillText(formatValue(vMin, output), 2, h - 12);

    // 十字光标 (与示波器一致: 金色虚线 + 读数标签)
    // Y 轴吸附: X 对齐到最近频点 bin, Y 取该 bin 的曲线值, 交点落在谱线上
    if (cursor) {
      const i = Math.max(0, Math.min(n - 1, Math.round((cursor.x / w) * (n - 1))));
      const snapX = (i / (n - 1)) * w;
      const snapValue = values[i];
      const snapY = h - ((snapValue - vMin) / (vMax - vMin)) * h;

      const cursorColor = getThemeColor('--color-waveform-cursor', '#ffd700');
      ctx.save();
      ctx.strokeStyle = cursorColor;
      ctx.lineWidth = 1;
      ctx.setLineDash([4, 2]);
      ctx.beginPath();
      ctx.moveTo(snapX, 0);
      ctx.lineTo(snapX, h);
      ctx.moveTo(0, snapY);
      ctx.lineTo(w, snapY);
      ctx.stroke();
      ctx.setLineDash([]);

      // 吸附点 — 实心圆标记谱线上的实际取值
      ctx.fillStyle = cursorColor;
      ctx.beginPath();
      ctx.arc(snapX, snapY, 3, 0, Math.PI * 2);
      ctx.fill();

      // 吸附点 频率 / 幅值 读数
      const freq = freqs[i] ?? (i / (n - 1)) * maxFreq;
      const label = `${formatFreq(freq)}  ${formatValue(snapValue, output)}`;
      ctx.font = '10px "JetBrains Mono", monospace';
      const textW = ctx.measureText(label).width;
      let lx = snapX + 10;
      let ly = snapY - 10;
      if (lx + textW + 8 > w) lx = snapX - textW - 18;
      if (ly - 13 < 0) ly = snapY + 20;
      ctx.fillStyle = 'rgba(38, 43, 52, 0.92)';
      ctx.fillRect(lx - 4, ly - 11, textW + 8, 15);
      ctx.strokeStyle = cursorColor;
      ctx.strokeRect(lx - 4, ly - 11, textW + 8, 15);
      ctx.fillStyle = '#d7dce4';
      ctx.textAlign = 'left';
      ctx.fillText(label, lx, ly);
      ctx.restore();
    }
  }, [result, sourceId, sampleRate, output, lang, size, cursor]);

  return (
    <div className="group widget-card-acrylic flex-1 min-w-0 min-h-0 flex relative overflow-hidden">
      {/* 主区: 频谱 Canvas 铺满 */}
      <div
        className="flex-1 min-w-0 min-h-0 relative bg-[#1e1e1e] cursor-crosshair"
        onMouseMove={(e) => {
          const rect = e.currentTarget.getBoundingClientRect();
          setCursor({ x: e.clientX - rect.left, y: e.clientY - rect.top });
        }}
        onMouseLeave={() => setCursor(null)}
      >
        <canvas
          ref={canvasRef}
          style={{ width: '100%', height: '100%', display: 'block' }}
        />
        {/* 状态标签覆盖左上角 */}
        <div className="absolute top-2 left-2 px-1.5 py-0.5 bg-accent/15 text-accent border border-accent/40 rounded-sm text-[10px] font-semibold pointer-events-none">
          {sourceWidget ? `${sourceWidget.params.label} · ${windowSize} · ${output}` : t(lang, 'spectrumNoSource')}
        </div>
        {onEdit && (
          <button
            className="absolute top-2 right-2 opacity-0 transition-opacity duration-150 group-hover:opacity-100 w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary bg-bg-scrim"
            onClick={onEdit}
            title={t(lang, 'settings')}
          >
            <Settings2 size={11} />
          </button>
        )}
      </div>
      {/* 侧栏: 设置面板 (固定宽, 纵向滚动, 直接展开) */}
      <div className="w-[240px] flex-shrink-0 border-l border-border bg-bg-sidebar overflow-y-auto flex flex-col gap-2 p-2.5">
        <div className="text-[10px] text-text-secondary uppercase tracking-wide font-semibold px-1">{t(lang, 'spectrumSettings')}</div>
        <div className="flex flex-col gap-1.5 px-1">
          <div className="grid grid-cols-[80px_1fr] items-center gap-1.5 text-[10px]">
            <label className="text-text-secondary">{t(lang, 'spectrumSource')}</label>
            <span className="font-mono text-text-primary truncate" title={sourceWidget?.params.label}>
              {sourceWidget ? sourceWidget.params.label : t(lang, 'spectrumNoSource')}
            </span>
          </div>
          {!sourceWidget && (
            <div className="text-[10px] text-text-secondary px-1">{t(lang, 'spectrumNoSourceHint')}</div>
          )}
          {sourceWidget && (
            <div className="flex flex-col gap-1 text-[10px] text-text-secondary px-1">
              <div className="flex justify-between">
                <span>{t(lang, 'spectrumWindowSize')}</span>
                <span className="font-mono text-text-primary">{sourceWidget.params.windowSize}</span>
              </div>
              <div className="flex justify-between">
                <span>{t(lang, 'spectrumWindowType')}</span>
                <span className="font-mono text-text-primary">{sourceWidget.params.windowType}</span>
              </div>
              <div className="flex justify-between">
                <span>{t(lang, 'spectrumOutputMode')}</span>
                <span className="font-mono text-text-primary">{sourceWidget.params.output}</span>
              </div>
              <div className="flex justify-between">
                <span>{t(lang, 'filterSampleRate')}</span>
                <span className="font-mono text-text-primary">{sourceWidget.params.sampleRate} Hz</span>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
});

/// 格式化频率 (Hz / kHz)
function formatFreq(hz: number): string {
  if (hz >= 1000) return (hz / 1000).toFixed(1) + 'k';
  if (hz >= 1) return hz.toFixed(0);
  return hz.toFixed(2);
}

/// 格式化频谱值 (根据输出模式)
function formatValue(v: number, output: SpectrumOutput): string {
  if (!Number.isFinite(v)) return '—';
  if (output === 'Decibel') return v.toFixed(1) + 'dB';
  if (Math.abs(v) >= 1000 || (Math.abs(v) < 0.01 && v !== 0)) {
    return v.toExponential(2);
  }
  return v.toFixed(3);
}
