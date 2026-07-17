import { Eye, EyeOff } from 'lucide-react';
import { StepKnob } from './StepKnob';
import {
  V_PER_DIV,
  formatVPerDiv,
  type ChannelAxisConfig,
  type Coupling,
} from '../../types';
import type { RenderStepSelect } from './scopeShared';

/// "全部" Tab 中的紧凑通道行 — 每通道一行: show/耦合/Vdiv旋钮+下拉/Position
export function CompactChannelRow({
  idx,
  ch,
  yUnit,
  onPatchChannel,
  renderStepSelect,
}: {
  idx: number;
  ch: ChannelAxisConfig;
  yUnit: string;
  onPatchChannel: (idx: number, p: Partial<ChannelAxisConfig>) => void;
  renderStepSelect: RenderStepSelect;
}) {
  const unit = yUnit ?? 'V';
  return (
    <div className="channel-row">
      <div className="channel-row-header">
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
      </div>
      <div className="scope-knob-row">
        <StepKnob
          value={ch.vPerDiv}
          steps={V_PER_DIV}
          onChange={(v) => onPatchChannel(idx, { vPerDiv: v })}
          defaultValue={1}
          formatValue={(v) => formatVPerDiv(v, unit)}
          size={40}
          disabled={!ch.show}
        />
        {renderStepSelect(
          V_PER_DIV,
          ch.vPerDiv,
          (v) => onPatchChannel(idx, { vPerDiv: v }),
          (v) => formatVPerDiv(v, unit)
        )}
      </div>
      <div className="scope-knob-row small">
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
        <span className="scope-unit">{unit}</span>
      </div>
    </div>
  );
}
