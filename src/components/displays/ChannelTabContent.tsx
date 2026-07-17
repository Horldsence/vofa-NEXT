import { Eye, EyeOff } from 'lucide-react';
import { StepKnob } from './StepKnob';
import { t, type Lang } from '../../i18n';
import {
  V_PER_DIV,
  formatVPerDiv,
  type ChannelAxisConfig,
  type Coupling,
} from '../../types';
import { CHANNEL_TAB_COLORS, type RenderStepSelect } from './scopeShared';

/// "CHn" 单通道 Tab — 仅显示该通道设置 (不含全局控件)
/// - header: 颜色条 + CH标签 + Show切换
/// - 耦合选择
/// - V/div 大旋钮 (72px) + 下拉
/// - Position 输入
export function ChannelTabContent({
  idx,
  ch,
  onPatchChannel,
  renderStepSelect,
  lang,
}: {
  idx: number;
  ch: ChannelAxisConfig;
  onPatchChannel: (idx: number, p: Partial<ChannelAxisConfig>) => void;
  renderStepSelect: RenderStepSelect;
  lang: Lang;
}) {
  return (
    <div className="scope-panel">
      <div className="scope-section scope-channel-header">
        <button
          className={`scope-channel-toggle ${ch.show ? 'on' : 'off'}`}
          onClick={() => onPatchChannel(idx, { show: !ch.show })}
          title={`CH${idx}`}
        >
          {ch.show ? <Eye size={12} /> : <EyeOff size={12} />}
          <span>CH{idx}</span>
        </button>
        <span
          className="scope-channel-color"
          style={{ background: CHANNEL_TAB_COLORS[idx % CHANNEL_TAB_COLORS.length] }}
        />
      </div>

      <div className="scope-section">
        <div className="scope-section-title">{t(lang, 'coupling')}</div>
        <select
          className="scope-select"
          value={ch.coupling}
          onChange={(e) =>
            onPatchChannel(idx, { coupling: e.target.value as Coupling })
          }
        >
          <option value="DC">DC</option>
          <option value="AC">AC</option>
          <option value="GND">GND</option>
        </select>
      </div>

      <div className="scope-section">
        <div className="scope-section-title">V/div</div>
        <div className="scope-knob-row standalone">
          <StepKnob
            value={ch.vPerDiv}
            steps={V_PER_DIV}
            onChange={(v) => onPatchChannel(idx, { vPerDiv: v })}
            defaultValue={1}
            formatValue={formatVPerDiv}
            size={72}
            disabled={!ch.show}
          />
        </div>
        <div className="scope-knob-row">
          {renderStepSelect(
            V_PER_DIV,
            ch.vPerDiv,
            (v) => onPatchChannel(idx, { vPerDiv: v }),
            formatVPerDiv
          )}
        </div>
      </div>

      <div className="scope-section">
        <div className="scope-section-title">{t(lang, 'position')}</div>
        <div className="scope-knob-row">
          <span className="scope-field-label">Pos</span>
          <input
            type="number"
            className="scope-number-input"
            value={ch.position}
            step={ch.vPerDiv}
            onChange={(e) =>
              onPatchChannel(idx, { position: parseFloat(e.target.value) || 0 })
            }
          />
          <span className="scope-unit">V</span>
        </div>
      </div>
    </div>
  );
}
