import { Play, Square, Zap, Crosshair } from 'lucide-react';
import { t, type Lang } from '../../i18n';
import {
  TIME_BASES_SEC,
  formatTimeBase,
  type ScopeAxisConfig,
  type ScopeMeasurements,
  type ChannelAxisConfig,
} from '../../types';
import { StepKnob } from './StepKnob';
import { CompactChannelRow } from './CompactChannelRow';
import { MeasureItem, formatFreq } from './MeasureItem';
import type { RenderStepSelect } from './scopeShared';

/// "全部" Tab — 全局控件 + 所有通道列表 + 游标 + 测量
export function AllTabContent({
  config,
  channels,
  measurements,
  onAutoSet,
  lang,
  patch,
  patchChannel,
  renderStepSelect,
}: {
  config: ScopeAxisConfig;
  channels: ChannelAxisConfig[];
  measurements?: ScopeMeasurements | null;
  onAutoSet?: () => void;
  lang: Lang;
  patch: (p: Partial<ScopeAxisConfig>) => void;
  patchChannel: (idx: number, p: Partial<ChannelAxisConfig>) => void;
  renderStepSelect: RenderStepSelect;
}) {
  return (
    <div className="scope-panel">
      <div className="scope-section scope-toolbar">
        <button
          className={`scope-btn ${config.running ? 'run' : 'stop'}`}
          onClick={() => patch({ running: !config.running })}
          title={config.running ? t(lang, 'stop') : t(lang, 'run')}
        >
          {config.running ? <Square size={12} /> : <Play size={12} />}
          <span>{config.running ? t(lang, 'stop') : t(lang, 'run')}</span>
        </button>
        <button
          className="scope-btn"
          onClick={() => onAutoSet?.()}
          title={t(lang, 'autoSet')}
          disabled={!onAutoSet}
        >
          <Zap size={12} />
          <span>{t(lang, 'autoSet')}</span>
        </button>
        <button
          className={`scope-btn icon-only ${config.grid ? 'active' : ''}`}
          onClick={() => patch({ grid: !config.grid })}
          title={t(lang, 'gridVisible')}
        >
          <Crosshair size={12} />
        </button>
      </div>

      {/* 水平 */}
      <div className="scope-section">
        <div className="scope-section-title">{t(lang, 'horizontal')}</div>
        <div className="scope-knob-row">
          <StepKnobTimeBase config={config} patch={patch} lang={lang} />
          {renderStepSelect(
            TIME_BASES_SEC,
            config.timeBase,
            (v) => patch({ timeBase: v }),
            formatTimeBase
          )}
        </div>
        <div className="scope-knob-row small">
          <span className="scope-field-label">{t(lang, 'hPosition')}</span>
          <input
            type="number"
            className="scope-number-input"
            value={config.hPosition}
            step={config.timeBase}
            onChange={(e) => patch({ hPosition: parseFloat(e.target.value) || 0 })}
          />
          <span className="scope-unit">s</span>
        </div>
      </div>

      {/* 每通道 */}
      <div className="scope-section">
        <div className="scope-section-title">{t(lang, 'channels')}</div>
        {channels.map((ch, idx) => (
          <CompactChannelRow
            key={idx}
            idx={idx}
            ch={ch}
            onPatchChannel={patchChannel}
            renderStepSelect={renderStepSelect}
          />
        ))}
      </div>

      {/* 游标 */}
      <CursorSection config={config} patch={patch} lang={lang} />

      {/* 测量值 */}
      {measurements && (
        <div className="scope-section">
          <div className="scope-section-title">{t(lang, 'measure')}</div>
          <div className="measure-grid">
            <MeasureItem label="Vpp" value={measurements.vpp} unit="V" />
            <MeasureItem label="Vmax" value={measurements.vmax} unit="V" />
            <MeasureItem label="Vmin" value={measurements.vmin} unit="V" />
            <MeasureItem label="Vavg" value={measurements.vavg} unit="V" />
            <MeasureItem label="Vrms" value={measurements.vrms} unit="V" />
            {measurements.freq != null && (
              <MeasureItem label="Freq" value={measurements.freq} unit="Hz" formatter={formatFreq} />
            )}
          </div>
        </div>
      )}
    </div>
  );
}

/// 时基旋钮 (提取为子组件以减少行数)
function StepKnobTimeBase({
  config,
  patch,
  lang,
}: {
  config: ScopeAxisConfig;
  patch: (p: Partial<ScopeAxisConfig>) => void;
  lang: Lang;
}) {
  return (
    <StepKnob
      value={config.timeBase}
      steps={TIME_BASES_SEC}
      onChange={(v: number) => patch({ timeBase: v })}
      defaultValue={100e-3}
      formatValue={formatTimeBase}
      label={t(lang, 'timeBase')}
      size={48}
    />
  );
}

/// 游标区
function CursorSection({
  config,
  patch,
  lang,
}: {
  config: ScopeAxisConfig;
  patch: (p: Partial<ScopeAxisConfig>) => void;
  lang: Lang;
}) {
  if (!config.cursors.enabled) {
    return (
      <div className="scope-section">
        <div className="scope-section-title">
          <label className="radio-item">
            <input
              type="checkbox"
              checked={false}
              onChange={(e) =>
                patch({ cursors: { ...config.cursors, enabled: e.target.checked } })
              }
            />
            <span>{t(lang, 'cursors')}</span>
          </label>
        </div>
      </div>
    );
  }
  const isVertical = config.cursors.type === 'vertical';
  const step = isVertical ? config.timeBase : 0.1;
  const unit = isVertical ? 's' : 'V';
  return (
    <div className="scope-section">
      <div className="scope-section-title">
        <label className="radio-item">
          <input
            type="checkbox"
            checked
            onChange={(e) =>
              patch({ cursors: { ...config.cursors, enabled: e.target.checked } })
            }
          />
          <span>{t(lang, 'cursors')}</span>
        </label>
      </div>
      <div className="scope-knob-row small">
        <select
          className="scope-select small"
          value={config.cursors.type}
          onChange={(e) =>
            patch({
              cursors: { ...config.cursors, type: e.target.value as 'vertical' | 'horizontal' },
            })
          }
        >
          <option value="vertical">X</option>
          <option value="horizontal">Y</option>
        </select>
      </div>
      <div className="scope-knob-row small">
        <span className="scope-field-label">C1</span>
        <input
          type="number"
          className="scope-number-input"
          value={config.cursors.c1}
          step={step}
          onChange={(e) =>
            patch({ cursors: { ...config.cursors, c1: parseFloat(e.target.value) || 0 } })
          }
        />
        <span className="scope-unit">{unit}</span>
      </div>
      <div className="scope-knob-row small">
        <span className="scope-field-label">C2</span>
        <input
          type="number"
          className="scope-number-input"
          value={config.cursors.c2}
          step={step}
          onChange={(e) =>
            patch({ cursors: { ...config.cursors, c2: parseFloat(e.target.value) || 0 } })
          }
        />
        <span className="scope-unit">{unit}</span>
      </div>
      <div className="scope-knob-row small">
        <span className="scope-field-label">Δ</span>
        <span className="scope-readout">
          {(config.cursors.c2 - config.cursors.c1).toFixed(4)}
          {unit}
        </span>
      </div>
    </div>
  );
}
