import { useState } from 'react';
import { ChevronDown, ChevronUp } from 'lucide-react';
import { WidgetCard } from '../ui/WidgetCard';
import type { WidgetConfig, FilterPresetKind } from '../../types';
import { useAppStore } from '../../store/appStore';
import { useGraphInput } from '../../lib/useGraphInput';
import { t } from '../../i18n';

interface FilterWidgetProps {
  widget: Extract<WidgetConfig, { kind: 'Filter' }>;
  onRemove: () => void;
  onEdit?: () => void;
}

/// 预设类型选项 (与 Rust FilterPreset 对应)
const PRESET_OPTIONS: { value: FilterPresetKind; labelKey: string }[] = [
  { value: 'Lowpass', labelKey: 'filterLowpass' },
  { value: 'Highpass', labelKey: 'filterHighpass' },
  { value: 'Bandpass', labelKey: 'filterBandpass' },
  { value: 'Bandstop', labelKey: 'filterBandstop' },
];

/// 滤波器控件 — 显示后端图评估的滤波结果
///
/// 数据流 (后端逐点滤波, 60 FPS 推送):
///   1. 后端 CompiledGraph 在 eval_order 中评估 Filter 节点:
///      - 取输入 "in0" 上游值 → DigitalFilter.process(value) → 输出端口 "result"
///      - 滤波器状态 (FIR 延迟线 / IIR biquad state) 跨帧持久化
///   2. 后端 graph_output_ticker 每 16ms 将所有节点输出快照推送至前端
///   3. 本组件直接读 graphOutputs[id].result 显示结果
///
/// 配置变更 (preset/cutoff/sampleRate) → updateWidget → syncTabGraph
/// → 后端重建 DigitalFilter (kind 变化触发状态重置, 符合滤波器语义)
export function FilterWidget({ widget, onEdit }: FilterWidgetProps) {
  const { preset, cutoff, low, high, sampleRate, precision, id } = widget.params;
  const graphOutputs = useAppStore((s) => s.graphOutputs);
  const updateWidget = useAppStore((s) => s.updateWidget);
  const lang = useAppStore((s) => s.lang);
  const [showSettings, setShowSettings] = useState(false);

  // 读取输入端口值 (用于显示)
  const inputValue = useGraphInput(id, 'in0', null, 0);
  // 后端滤波后的结果
  const result = graphOutputs[id]?.result ?? 0;

  const handlePresetChange = (newPreset: FilterPresetKind) => {
    updateWidget(id, {
      kind: 'Filter',
      params: { ...widget.params, preset: newPreset },
    });
  };

  const handleNumberChange = (field: 'cutoff' | 'low' | 'high' | 'sampleRate', value: string) => {
    const num = parseFloat(value);
    if (!Number.isFinite(num) || num <= 0) return;
    updateWidget(id, {
      kind: 'Filter',
      params: { ...widget.params, [field]: num },
    });
  };

  const presetLabel = t(lang, PRESET_OPTIONS.find((o) => o.value === preset)?.labelKey ?? 'filterLowpass');

  return (
    <WidgetCard badge={presetLabel} badgeColor="orange" className="border-[#ff8c42]" onEdit={onEdit}>
      <div className="flex flex-col gap-1 px-1.5 py-1">
        <div className="flex items-baseline justify-center gap-1 py-1">
          <span className="text-[22px] font-semibold text-[#ff8c42] font-mono">
            {result.toFixed(precision)}
          </span>
        </div>
        <div className="flex justify-between items-center text-xs px-1 py-0.5 bg-bg-subtle rounded-sm">
          <span className="text-text-secondary">in</span>
          <span className="text-text-primary font-mono">{inputValue.toFixed(precision)}</span>
        </div>
        <button
          className="flex items-center justify-center gap-1 bg-transparent border border-border text-text-secondary px-1.5 py-0.5 rounded-sm text-[10px] cursor-pointer mt-0.5 hover:bg-bg-hover hover:text-text-primary transition-colors"
          onClick={() => setShowSettings((v) => !v)}
          title={t(lang, 'settings')}
        >
          {showSettings ? <ChevronUp size={10} /> : <ChevronDown size={10} />}
          <span>{t(lang, 'filterSettings')}</span>
        </button>
        {showSettings && (
          <div className="flex flex-col gap-1.5 p-1.5 bg-bg-scrim rounded-sm border border-border">
            <div className="grid grid-cols-[60px_1fr_auto] items-center gap-1.5 text-[10px]">
              <label className="text-text-secondary">{t(lang, 'filterPreset')}</label>
              <div className="grid grid-cols-2 gap-0.5 col-span-2">
                {PRESET_OPTIONS.map((opt) => (
                  <button
                    key={opt.value}
                    className={`px-1 py-0.5 bg-bg-input border border-border rounded-sm text-text-secondary text-[10px] cursor-pointer transition-colors hover:border-[#ff8c42] hover:text-[#ff8c42] ${preset === opt.value ? 'bg-[#ff8c42]/20 border-[#ff8c42] text-[#ff8c42]' : ''}`}
                    onClick={() => handlePresetChange(opt.value)}
                  >
                    {t(lang, opt.labelKey)}
                  </button>
                ))}
              </div>
            </div>
            {(preset === 'Lowpass' || preset === 'Highpass') && (
              <div className="grid grid-cols-[60px_1fr_auto] items-center gap-1.5 text-[10px]">
                <label className="text-text-secondary">{t(lang, 'filterCutoff')}</label>
                <input
                  type="number"
                  value={cutoff}
                  onChange={(e) => handleNumberChange('cutoff', e.target.value)}
                  min={0}
                  step={1}
                  className="w-full px-1 py-0.5 bg-bg-input border border-border rounded-sm text-text-primary text-xs font-mono focus:outline-none focus:border-accent"
                />
                <span className="text-text-secondary text-[10px]">Hz</span>
              </div>
            )}
            {(preset === 'Bandpass' || preset === 'Bandstop') && (
              <>
                <div className="grid grid-cols-[60px_1fr_auto] items-center gap-1.5 text-[10px]">
                  <label className="text-text-secondary">{t(lang, 'filterLow')}</label>
                  <input
                    type="number"
                    value={low}
                    onChange={(e) => handleNumberChange('low', e.target.value)}
                    min={0}
                    step={1}
                    className="w-full px-1 py-0.5 bg-bg-input border border-border rounded-sm text-text-primary text-xs font-mono focus:outline-none focus:border-accent"
                  />
                  <span className="text-text-secondary text-[10px]">Hz</span>
                </div>
                <div className="grid grid-cols-[60px_1fr_auto] items-center gap-1.5 text-[10px]">
                  <label className="text-text-secondary">{t(lang, 'filterHigh')}</label>
                  <input
                    type="number"
                    value={high}
                    onChange={(e) => handleNumberChange('high', e.target.value)}
                    min={0}
                    step={1}
                    className="w-full px-1 py-0.5 bg-bg-input border border-border rounded-sm text-text-primary text-xs font-mono focus:outline-none focus:border-accent"
                  />
                  <span className="text-text-secondary text-[10px]">Hz</span>
                </div>
              </>
            )}
            <div className="grid grid-cols-[60px_1fr_auto] items-center gap-1.5 text-[10px]">
              <label className="text-text-secondary">{t(lang, 'filterSampleRate')}</label>
              <input
                type="number"
                value={sampleRate}
                onChange={(e) => handleNumberChange('sampleRate', e.target.value)}
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
}
