import { memo } from 'react';
import { WidgetCard } from '../../ui/WidgetCard';
import type { WidgetConfig, SpectrumOutput } from '../../../types';
import { useAppStore } from '../../../store/appStore';
import { useNumericInput } from '../../../lib/hooks/useNumericPort';
import { t } from '../../../i18n';

interface FFTWidgetProps {
  widget: Extract<WidgetConfig, { kind: 'FFT' }>;
  onRemove: () => void;
  onEdit?: () => void;
}

/// 输出模式选项 (badge 摘要用; 编辑入口在节点属性面板)
const OUTPUT_LABEL: Record<SpectrumOutput, string> = {
  Magnitude: 'spectrumMagnitude',
  Power: 'spectrumPower',
  PSD: 'spectrumPSD',
  Decibel: 'spectrumDecibel',
};

/// FFT 频域求解器 — 输入时域信号 in0, 输出频谱到专用频谱数据通道
///
/// 数据流 (与旧 Spectrum 求解路径一致, 但求解器与展示分离):
///   1. 本控件映射为后端 SpectrumSink 节点, 逐帧消费 in0 的时域值
///   2. 后端 SpectrumAnalyzer 维护滑动窗口, 30 FPS 触发 FFT
///   3. 结果以本 widget id 为 key 存入 spectrumResults
///   4. 下游「频谱」展示控件选择本求解器 id 作为数据源读取并绘制
///
/// 设置 (windowSize/windowType/output/sampleRate) 在节点属性面板编辑,
/// 卡片内只保留主峰频率与输入值显示。
export const FFTWidget = memo(function FFTWidget({ widget, onEdit }: FFTWidgetProps) {
  const { windowSize, output, id } = widget.params;
  const result = useAppStore((s) => s.spectrumResults[id]);
  const lang = useAppStore((s) => s.lang);

  // 输入端口值 (时域) — 用于显示
  const inputValue = useNumericInput(id, 'in0').latest?.value ?? 0;

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

  const badge = `${windowSize} · ${t(lang, OUTPUT_LABEL[output] ?? 'spectrumMagnitude')}`;

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
