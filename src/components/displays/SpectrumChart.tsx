import { useEffect, useRef, useState } from 'react';
import { Settings2 } from 'lucide-react';
import type { WidgetConfig, WindowType, SpectrumOutput, SpectrumResult } from '../../types';
import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';

interface SpectrumChartProps {
  widget: Extract<WidgetConfig, { kind: 'Spectrum' }>;
  onRemove: () => void;
  onEdit?: () => void;
}

/// 窗口大小选项 (2 的幂)
const WINDOW_SIZE_OPTIONS = [256, 512, 1024, 2048, 4096];

/// 窗函数选项
const WINDOW_TYPE_OPTIONS: { value: WindowType; labelKey: string }[] = [
  { value: 'Rect', labelKey: 'windowRect' },
  { value: 'Hann', labelKey: 'windowHann' },
  { value: 'Hamming', labelKey: 'windowHamming' },
  { value: 'Blackman', labelKey: 'windowBlackman' },
];

/// 输出模式选项
const OUTPUT_OPTIONS: { value: SpectrumOutput; labelKey: string }[] = [
  { value: 'Magnitude', labelKey: 'spectrumMagnitude' },
  { value: 'Power', labelKey: 'spectrumPower' },
  { value: 'PSD', labelKey: 'spectrumPSD' },
  { value: 'Decibel', labelKey: 'spectrumDecibel' },
];

/// 频谱分析控件 — 显示后端 FFT 计算结果
///
/// 数据流 (后端块运算, 30 FPS 推送):
///   1. 后端 CompiledGraph 不评估 SpectrumSink (不在 eval_order)
///   2. evaluate_all_graphs_with 每帧后调用 collect_spectrum_inputs,
///      从 output_snapshot 取输入值推入对应 SpectrumAnalyzer 的滑动窗口
///   3. spectrum_ticker 每 33ms:
///      - sync_spectrum_analyzers 同步 analyzers 与 graphs (增删 + 配置变更重建)
///      - 对每个 analyzer 调用 compute() (窗口未填满返回 None)
///      - 推送 SpectrumBatch 到所有订阅者
///   4. 本组件从 store.spectrumResults[id] 读取最新结果并绘制
export function SpectrumChart({ widget, onEdit }: SpectrumChartProps) {
  const { windowSize, windowType, output, sampleRate, id } = widget.params;
  const spectrumResults = useAppStore((s) => s.spectrumResults);
  const updateWidget = useAppStore((s) => s.updateWidget);
  const lang = useAppStore((s) => s.lang);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  // 跟踪容器尺寸, 触发 canvas 重绘 (响应式)
  const [size, setSize] = useState({ w: 0, h: 0 });

  const result: SpectrumResult | undefined = spectrumResults[id];

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
  }, [result, sampleRate, output, lang, size]);

  const handleWindowSizeChange = (sz: number) => {
    updateWidget(id, {
      kind: 'Spectrum',
      params: { ...widget.params, windowSize: sz },
    });
  };

  const handleWindowTypeChange = (wt: WindowType) => {
    updateWidget(id, {
      kind: 'Spectrum',
      params: { ...widget.params, windowType: wt },
    });
  };

  const handleOutputChange = (o: SpectrumOutput) => {
    updateWidget(id, {
      kind: 'Spectrum',
      params: { ...widget.params, output: o },
    });
  };

  const handleSampleRateChange = (value: string) => {
    const num = parseFloat(value);
    if (!Number.isFinite(num) || num <= 0) return;
    updateWidget(id, {
      kind: 'Spectrum',
      params: { ...widget.params, sampleRate: num },
    });
  };

  return (
    <div className="group bg-bg-sidebar border border-[#81c784] rounded flex-1 min-w-0 min-h-0 flex relative overflow-hidden">
      {/* 主区: 频谱 Canvas 铺满 */}
      <div className="flex-1 min-w-0 min-h-0 relative bg-[#1e1e1e]">
        <canvas
          ref={canvasRef}
          style={{ width: '100%', height: '100%', display: 'block' }}
        />
        {/* 状态标签覆盖左上角 */}
        <div className="absolute top-2 left-2 px-1.5 py-0.5 bg-[#81c784]/15 text-[#81c784] border border-[#81c784]/40 rounded-sm text-[10px] font-semibold pointer-events-none">
          {windowSize} · {output}
        </div>
        {onEdit && (
          <button
            className="absolute top-2 right-2 opacity-0 transition-opacity duration-150 group-hover:opacity-100 w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary bg-black/40"
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
        <div className="flex flex-col gap-1.5 p-1.5 bg-black/20 rounded-sm border border-border">
            <div className="grid grid-cols-[80px_1fr_auto] items-center gap-1.5 text-[10px]">
              <label className="text-text-secondary">{t(lang, 'spectrumWindowSize')}</label>
              <div className="flex flex-wrap gap-0.5 col-span-2">
                {WINDOW_SIZE_OPTIONS.map((sz) => (
                  <button
                    key={sz}
                    className={`px-1.5 py-0.5 bg-bg-input border border-border rounded-sm text-text-secondary text-[10px] cursor-pointer transition-colors hover:border-[#81c784] hover:text-[#81c784] ${windowSize === sz ? 'bg-[#81c784]/20 border-[#81c784] text-[#81c784]' : ''}`}
                    onClick={() => handleWindowSizeChange(sz)}
                  >
                    {sz}
                  </button>
                ))}
              </div>
            </div>
            <div className="grid grid-cols-[80px_1fr_auto] items-center gap-1.5 text-[10px]">
              <label className="text-text-secondary">{t(lang, 'spectrumWindowType')}</label>
              <div className="flex flex-wrap gap-0.5 col-span-2">
                {WINDOW_TYPE_OPTIONS.map((opt) => (
                  <button
                    key={opt.value}
                    className={`px-1.5 py-0.5 bg-bg-input border border-border rounded-sm text-text-secondary text-[10px] cursor-pointer transition-colors hover:border-[#81c784] hover:text-[#81c784] ${windowType === opt.value ? 'bg-[#81c784]/20 border-[#81c784] text-[#81c784]' : ''}`}
                    onClick={() => handleWindowTypeChange(opt.value)}
                  >
                    {t(lang, opt.labelKey)}
                  </button>
                ))}
              </div>
            </div>
            <div className="grid grid-cols-[80px_1fr_auto] items-center gap-1.5 text-[10px]">
              <label className="text-text-secondary">{t(lang, 'spectrumOutputMode')}</label>
              <div className="flex flex-wrap gap-0.5 col-span-2">
                {OUTPUT_OPTIONS.map((opt) => (
                  <button
                    key={opt.value}
                    className={`px-1.5 py-0.5 bg-bg-input border border-border rounded-sm text-text-secondary text-[10px] cursor-pointer transition-colors hover:border-[#81c784] hover:text-[#81c784] ${output === opt.value ? 'bg-[#81c784]/20 border-[#81c784] text-[#81c784]' : ''}`}
                    onClick={() => handleOutputChange(opt.value)}
                  >
                    {t(lang, opt.labelKey)}
                  </button>
                ))}
              </div>
            </div>
            <div className="grid grid-cols-[80px_1fr_auto] items-center gap-1.5 text-[10px]">
              <label className="text-text-secondary">{t(lang, 'filterSampleRate')}</label>
              <input
                type="number"
                value={sampleRate}
                onChange={(e) => handleSampleRateChange(e.target.value)}
                min={1}
                step={1}
                className="w-full px-1 py-0.5 bg-bg-input border border-border rounded-sm text-text-primary text-xs font-mono focus:outline-none focus:border-accent"
              />
              <span className="text-text-secondary text-[10px]">Hz</span>
            </div>
          </div>
      </div>
    </div>
  );
}



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
