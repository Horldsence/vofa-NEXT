import { useState } from 'react';
import { Settings2, ChevronDown, ChevronUp } from 'lucide-react';
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
    <div className="widget-card filter-widget">
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
      <div className="filter-widget-preset-badge">{presetLabel}</div>
      <div className="filter-widget-body">
        <div className="filter-widget-result">
          <span className="filter-widget-result-value">
            {result.toFixed(precision)}
          </span>
        </div>
        <div className="filter-widget-input-row">
          <span className="filter-widget-input-label">in</span>
          <span className="filter-widget-input-value">{inputValue.toFixed(precision)}</span>
        </div>
        <button
          className="filter-widget-toggle"
          onClick={() => setShowSettings((v) => !v)}
          title={t(lang, 'settings')}
        >
          {showSettings ? <ChevronUp size={10} /> : <ChevronDown size={10} />}
          <span>{t(lang, 'filterSettings')}</span>
        </button>
        {showSettings && (
          <div className="filter-widget-settings">
            <div className="filter-widget-setting-row">
              <label>{t(lang, 'filterPreset')}</label>
              <div className="filter-widget-preset-buttons">
                {PRESET_OPTIONS.map((opt) => (
                  <button
                    key={opt.value}
                    className={`filter-widget-preset-btn ${preset === opt.value ? 'active' : ''}`}
                    onClick={() => handlePresetChange(opt.value)}
                  >
                    {t(lang, opt.labelKey)}
                  </button>
                ))}
              </div>
            </div>
            {(preset === 'Lowpass' || preset === 'Highpass') && (
              <div className="filter-widget-setting-row">
                <label>{t(lang, 'filterCutoff')}</label>
                <input
                  type="number"
                  value={cutoff}
                  onChange={(e) => handleNumberChange('cutoff', e.target.value)}
                  min={0}
                  step={1}
                />
                <span className="filter-widget-unit">Hz</span>
              </div>
            )}
            {(preset === 'Bandpass' || preset === 'Bandstop') && (
              <>
                <div className="filter-widget-setting-row">
                  <label>{t(lang, 'filterLow')}</label>
                  <input
                    type="number"
                    value={low}
                    onChange={(e) => handleNumberChange('low', e.target.value)}
                    min={0}
                    step={1}
                  />
                  <span className="filter-widget-unit">Hz</span>
                </div>
                <div className="filter-widget-setting-row">
                  <label>{t(lang, 'filterHigh')}</label>
                  <input
                    type="number"
                    value={high}
                    onChange={(e) => handleNumberChange('high', e.target.value)}
                    min={0}
                    step={1}
                  />
                  <span className="filter-widget-unit">Hz</span>
                </div>
              </>
            )}
            <div className="filter-widget-setting-row">
              <label>{t(lang, 'filterSampleRate')}</label>
              <input
                type="number"
                value={sampleRate}
                onChange={(e) => handleNumberChange('sampleRate', e.target.value)}
                min={1}
                step={1}
              />
              <span className="filter-widget-unit">Hz</span>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
