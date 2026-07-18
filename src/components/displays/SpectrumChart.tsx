import { useEffect, useRef, useState } from 'react';
import { Settings2, ChevronDown, ChevronUp } from 'lucide-react';
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
  const [showSettings, setShowSettings] = useState(false);
  const canvasRef = useRef<HTMLCanvasElement>(null);

  const result: SpectrumResult | undefined = spectrumResults[id];

  // 绘制频谱图
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.getBoundingClientRect();
    canvas.width = rect.width * dpr;
    canvas.height = rect.height * dpr;
    ctx.scale(dpr, dpr);

    const w = rect.width;
    const h = rect.height;
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
  }, [result, sampleRate, output, lang]);

  const handleWindowSizeChange = (size: number) => {
    updateWidget(id, {
      kind: 'Spectrum',
      params: { ...widget.params, windowSize: size },
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
    <div className="widget-card spectrum-widget">
      {onEdit && (
        <button
          className="btn-icon widget-edit"
          onClick={onEdit}
          title={t(lang, 'settings')}
          style={{ right: 24 }}
        >
          <Settings2 size={11} />
        </button>
      )}
      <div className="spectrum-widget-info">
        {windowSize} · {output}
      </div>
      <div className="spectrum-widget-body">
        <canvas
          ref={canvasRef}
          style={{ width: '100%', height: 110, background: '#1e1e1e' }}
        />
        <button
          className="spectrum-widget-toggle"
          onClick={() => setShowSettings((v) => !v)}
          title={t(lang, 'settings')}
        >
          {showSettings ? <ChevronUp size={10} /> : <ChevronDown size={10} />}
          <span>{t(lang, 'spectrumSettings')}</span>
        </button>
        {showSettings && (
          <div className="spectrum-widget-settings">
            <div className="spectrum-widget-setting-row">
              <label>{t(lang, 'spectrumWindowSize')}</label>
              <div className="spectrum-widget-btn-group">
                {WINDOW_SIZE_OPTIONS.map((size) => (
                  <button
                    key={size}
                    className={`spectrum-widget-btn ${windowSize === size ? 'active' : ''}`}
                    onClick={() => handleWindowSizeChange(size)}
                  >
                    {size}
                  </button>
                ))}
              </div>
            </div>
            <div className="spectrum-widget-setting-row">
              <label>{t(lang, 'spectrumWindowType')}</label>
              <div className="spectrum-widget-btn-group">
                {WINDOW_TYPE_OPTIONS.map((opt) => (
                  <button
                    key={opt.value}
                    className={`spectrum-widget-btn ${windowType === opt.value ? 'active' : ''}`}
                    onClick={() => handleWindowTypeChange(opt.value)}
                  >
                    {t(lang, opt.labelKey)}
                  </button>
                ))}
              </div>
            </div>
            <div className="spectrum-widget-setting-row">
              <label>{t(lang, 'spectrumOutputMode')}</label>
              <div className="spectrum-widget-btn-group">
                {OUTPUT_OPTIONS.map((opt) => (
                  <button
                    key={opt.value}
                    className={`spectrum-widget-btn ${output === opt.value ? 'active' : ''}`}
                    onClick={() => handleOutputChange(opt.value)}
                  >
                    {t(lang, opt.labelKey)}
                  </button>
                ))}
              </div>
            </div>
            <div className="spectrum-widget-setting-row">
              <label>{t(lang, 'filterSampleRate')}</label>
              <input
                type="number"
                value={sampleRate}
                onChange={(e) => handleSampleRateChange(e.target.value)}
                min={1}
                step={1}
              />
              <span className="spectrum-widget-unit">Hz</span>
            </div>
          </div>
        )}
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
