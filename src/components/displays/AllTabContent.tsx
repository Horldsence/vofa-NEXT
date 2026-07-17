import { Play, Square, Zap, Crosshair, Eye, EyeOff } from 'lucide-react';
import { t, type Lang } from '../../i18n';
import {
  TIME_BASES_SEC,
  V_PER_DIV,
  formatTimeBase,
  formatVPerDiv,
  type ScopeAxisConfig,
  type ScopeMeasurements,
  type ChannelAxisConfig,
  type Coupling,
} from '../../types';
import { StepKnob } from './StepKnob';
import { CompactChannelRow } from './CompactChannelRow';
import { MeasureItem, formatFreq } from './MeasureItem';
import { CHANNEL_TAB_COLORS, type RenderStepSelect } from './scopeShared';

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

      {/* 每通道 — 含 独立/共用 Y 切换 */}
      <div className="scope-section">
        <div className="scope-section-title scope-section-title-row">
          <span>{t(lang, 'channels')}</span>
          <button
            className={`scope-toggle-btn ${config.sharedY ? 'active' : ''}`}
            onClick={() => patch({ sharedY: !config.sharedY })}
            title={t(lang, 'sharedYDesc')}
          >
            {config.sharedY ? t(lang, 'sharedY') : t(lang, 'independentY')}
          </button>
        </div>
        {config.sharedY ? (
          <SharedYControls
            channels={channels}
            yUnit={config.yUnit}
            onPatchChannel={patchChannel}
            renderStepSelect={renderStepSelect}
            lang={lang}
          />
        ) : (
          channels.map((ch, idx) => (
            <CompactChannelRow
              key={idx}
              idx={idx}
              ch={ch}
              yUnit={config.yUnit}
              onPatchChannel={patchChannel}
              renderStepSelect={renderStepSelect}
            />
          ))
        )}
      </div>

      {/* Y 轴单位 */}
      <div className="scope-section">
        <div className="scope-section-title">{t(lang, 'yUnit')}</div>
        <div className="scope-knob-row small">
          <span className="scope-field-label">Unit</span>
          <input
            type="text"
            className="scope-number-input"
            value={config.yUnit}
            placeholder="V / A / °C / ''"
            onChange={(e) => patch({ yUnit: e.target.value })}
          />
        </div>
      </div>

      {/* 游标 */}
      <CursorSection config={config} patch={patch} lang={lang} />

      {/* 测量值 */}
      {measurements && (
        <div className="scope-section">
          <div className="scope-section-title">{t(lang, 'measure')}</div>
          <div className="measure-grid">
            <MeasureItem label="PP" value={measurements.vpp} unit={config.yUnit} />
            <MeasureItem label="Max" value={measurements.vmax} unit={config.yUnit} />
            <MeasureItem label="Min" value={measurements.vmin} unit={config.yUnit} />
            <MeasureItem label="Avg" value={measurements.vavg} unit={config.yUnit} />
            <MeasureItem label="RMS" value={measurements.vrms} unit={config.yUnit} />
            {measurements.freq != null && (
              <MeasureItem label="Freq" value={measurements.freq} unit="Hz" formatter={formatFreq} />
            )}
          </div>
        </div>
      )}
    </div>
  );
}

/// 共用 Y 模式 — 单组 V/div/Position 控件 (操作 channels[0]) + 每通道 show/耦合
function SharedYControls({
  channels,
  yUnit,
  onPatchChannel,
  renderStepSelect,
  lang,
}: {
  channels: ChannelAxisConfig[];
  yUnit: string;
  onPatchChannel: (idx: number, p: Partial<ChannelAxisConfig>) => void;
  renderStepSelect: RenderStepSelect;
  lang: Lang;
}) {
  const unit = yUnit ?? '';
  // 共用 Y: 所有通道共享 channels[0] 的 vPerDiv/position
  const shared = channels[0] ?? { vPerDiv: 1, position: 0, show: true, coupling: 'DC' as Coupling };
  return (
    <>
      {/* 共用 V/div 旋钮 + 下拉 (操作 channels[0], 影响所有通道) */}
      <div className="scope-knob-row standalone">
        <StepKnob
          value={shared.vPerDiv}
          steps={V_PER_DIV}
          onChange={(v) => onPatchChannel(0, { vPerDiv: v })}
          defaultValue={1}
          formatValue={(v) => formatVPerDiv(v, unit)}
          label={`${unit || 'Y'}/div`}
          size={48}
        />
        {renderStepSelect(
          V_PER_DIV,
          shared.vPerDiv,
          (v) => onPatchChannel(0, { vPerDiv: v }),
          (v) => formatVPerDiv(v, unit)
        )}
      </div>
      <div className="scope-knob-row small">
        <span className="scope-field-label">{t(lang, 'position')}</span>
        <input
          type="number"
          className="scope-number-input"
          value={shared.position}
          step={shared.vPerDiv}
          onChange={(e) => onPatchChannel(0, { position: parseFloat(e.target.value) || 0 })}
        />
        <span className="scope-unit">{unit}</span>
      </div>
      {/* 每通道 show + 耦合 (per-channel, 不共用) */}
      {channels.map((ch, idx) => (
        <ChannelVisibilityRow
          key={idx}
          idx={idx}
          ch={ch}
          onPatchChannel={onPatchChannel}
        />
      ))}
    </>
  );
}

/// 共用 Y 模式下的单通道行 — 仅 show 切换 + 耦合 (不含 V/div/Position)
function ChannelVisibilityRow({
  idx,
  ch,
  onPatchChannel,
}: {
  idx: number;
  ch: ChannelAxisConfig;
  onPatchChannel: (idx: number, p: Partial<ChannelAxisConfig>) => void;
}) {
  return (
    <div className="channel-row channel-row-visibility">
      <button
        className={`scope-channel-toggle ${ch.show ? 'on' : 'off'}`}
        onClick={() => onPatchChannel(idx, { show: !ch.show })}
        title={`CH${idx}`}
      >
        {ch.show ? <Eye size={11} /> : <EyeOff size={11} />}
        <span>CH{idx}</span>
      </button>
      <select
        className="scope-select small"
        value={ch.coupling}
        onChange={(e) =>
          onPatchChannel(idx, { coupling: e.target.value as Coupling })
        }
      >
        <option value="DC">DC</option>
        <option value="AC">AC</option>
        <option value="GND">GND</option>
      </select>
      <span
        className="scope-tab-dot"
        style={{ background: CHANNEL_TAB_COLORS[idx % CHANNEL_TAB_COLORS.length] }}
      />
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
  // 水平游标用 Y 轴单位 (不一定是 V)
  const unit = isVertical ? 's' : (config.yUnit ?? 'V');
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
