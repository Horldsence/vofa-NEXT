import { memo, useState } from 'react';
import { ChevronDown, ChevronUp } from 'lucide-react';
import { WidgetCard } from '../../ui/WidgetCard';
import { chipClass } from '../../ui/chip';
import type { WidgetConfig, WindowType, SpectrumOutput } from '../../../types';
import { useAppStore } from '../../../store/appStore';
import { useNumericInput } from '../../../lib/hooks/useNumericPort';
import { t } from '../../../i18n';

interface FFTWidgetProps {
  widget: Extract<WidgetConfig, { kind: 'FFT' }>;
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

/// FFT 频域求解器 — 输入时域信号 in0, 输出频谱到专用频谱数据通道
///
/// 数据流 (与旧 Spectrum 求解路径一致, 但求解器与展示分离):
///   1. 本控件映射为后端 SpectrumSink 节点, 逐帧消费 in0 的时域值
///   2. 后端 SpectrumAnalyzer 维护滑动窗口, 30 FPS 触发 FFT
///   3. 结果以本 widget id 为 key 存入 spectrumResults
///   4. 下游「频谱」展示控件选择本求解器 id 作为数据源读取并绘制
export const FFTWidget = memo(function FFTWidget({ widget, onEdit }: FFTWidgetProps) {
  const { windowSize, windowType, output, sampleRate, id } = widget.params;
  const result = useAppStore((s) => s.spectrumResults[id]);
  const updateWidget = useAppStore((s) => s.updateWidget);
  const lang = useAppStore((s) => s.lang);
  const [showSettings, setShowSettings] = useState(false);

  // 输入端口值 (时域) — 用于显示
  const inputValue = useNumericInput(id, 'in0').latest?.value ?? 0;

  const handleChange = <K extends 'windowSize' | 'windowType' | 'output' | 'sampleRate'>(
    field: K,
    value: FFTWidgetProps['widget']['params'][K]
  ) => {
    updateWidget(id, {
      kind: 'FFT',
      params: { ...widget.params, [field]: value },
    });
  };

  const handleSampleRateChange = (value: string) => {
    const num = parseFloat(value);
    if (!Number.isFinite(num) || num <= 0) return;
    handleChange('sampleRate', num);
  };

  // 主峰 (频率, 幅值) — 展示求解器已产出频谱时给出可读反馈
  const peak = (() => {
    if (!result || result.values.length === 0) return null;
    const { values, frequencies } = result;
    let idx = 0;
    for (let i = 1; i < values.length; i++) {
      if (values[i] > values[idx]) idx = i;
    }
    return { freq: frequencies[idx] ?? 0, value: values[idx] };
  })();

  const badge = `${windowSize} · ${t(lang, OUTPUT_OPTIONS.find((o) => o.value === output)?.labelKey ?? 'spectrumMagnitude')}`;

  return (
    <WidgetCard badge={badge} badgeColor="purple" className="border-[#ba68c8]" onEdit={onEdit}>
      <div className="flex flex-col gap-1 px-1.5 py-1">
        <div className="flex items-baseline justify-center gap-1 py-1">
          {peak ? (
            <span className="text-[15px] font-semibold text-[#ba68c8] font-mono">
              {formatFreq(peak.freq)}
            </span>
          ) : (
            <span className="text-[11px] text-text-secondary font-mono py-0.5">
              {t(lang, 'spectrumWaiting')}
            </span>
          )}
        </div>
        <div className="flex justify-between items-center text-xs px-1 py-0.5 bg-bg-subtle rounded-sm">
          <span className="text-text-secondary">in</span>
          <span className="text-text-primary font-mono">{inputValue.toFixed(3)}</span>
        </div>
        <button
          className="flex items-center justify-center gap-1 bg-transparent border border-border text-text-secondary px-1.5 py-0.5 rounded-sm text-[10px] cursor-pointer mt-0.5 hover:bg-bg-hover hover:text-text-primary transition-colors"
          onClick={() => setShowSettings((v) => !v)}
          title={t(lang, 'settings')}
        >
          {showSettings ? <ChevronUp size={10} /> : <ChevronDown size={10} />}
          <span>{t(lang, 'fftSettings')}</span>
        </button>
        {showSettings && (
          <div className="flex flex-col gap-1.5 p-1.5 bg-bg-scrim rounded-sm border border-border">
            <div className="grid grid-cols-[60px_1fr] items-center gap-1.5 text-[10px]">
              <label className="text-text-secondary">{t(lang, 'spectrumWindowSize')}</label>
              <div className="flex flex-wrap gap-0.5">
                {WINDOW_SIZE_OPTIONS.map((sz) => (
                  <button
                    key={sz}
                    className={chipClass(windowSize === sz)}
                    onClick={() => handleChange('windowSize', sz)}
                  >
                    {sz}
                  </button>
                ))}
              </div>
            </div>
            <div className="grid grid-cols-[60px_1fr] items-center gap-1.5 text-[10px]">
              <label className="text-text-secondary">{t(lang, 'spectrumWindowType')}</label>
              <div className="flex flex-wrap gap-0.5">
                {WINDOW_TYPE_OPTIONS.map((opt) => (
                  <button
                    key={opt.value}
                    className={chipClass(windowType === opt.value)}
                    onClick={() => handleChange('windowType', opt.value)}
                  >
                    {t(lang, opt.labelKey)}
                  </button>
                ))}
              </div>
            </div>
            <div className="grid grid-cols-[60px_1fr] items-center gap-1.5 text-[10px]">
              <label className="text-text-secondary">{t(lang, 'spectrumOutputMode')}</label>
              <div className="flex flex-wrap gap-0.5">
                {OUTPUT_OPTIONS.map((opt) => (
                  <button
                    key={opt.value}
                    className={chipClass(output === opt.value)}
                    onClick={() => handleChange('output', opt.value)}
                  >
                    {t(lang, opt.labelKey)}
                  </button>
                ))}
              </div>
            </div>
            <div className="grid grid-cols-[60px_1fr_auto] items-center gap-1.5 text-[10px]">
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
        )}
      </div>
    </WidgetCard>
  );
});

/// 格式化频率 (Hz / kHz)
function formatFreq(hz: number): string {
  if (hz >= 1000) return (hz / 1000).toFixed(1) + 'kHz';
  if (hz >= 1) return hz.toFixed(1) + 'Hz';
  return hz.toFixed(2) + 'Hz';
}
