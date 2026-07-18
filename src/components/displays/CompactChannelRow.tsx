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
    <div className="flex flex-col gap-0.5 py-1 border-t border-border/50 first:border-t-0">
      <div className="flex items-center justify-between gap-1">
        <button
          className={`inline-flex items-center gap-1 border rounded px-1.5 py-0.5 text-[10px] font-mono cursor-pointer transition-all duration-150 ${ch.show ? 'text-text-bright border-blue bg-blue/10' : 'text-text-secondary border-border opacity-60'} hover:bg-bg-hover`}
          onClick={() => onPatchChannel(idx, { show: !ch.show })}
          title={`CH${idx}`}
        >
          {ch.show ? <Eye size={11} /> : <EyeOff size={11} />}
          <span>CH{idx}</span>
        </button>
        <select
          className="form-select w-auto flex-none text-[10px] py-0.5 px-1"
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
      <div className="flex items-center gap-1.5 mt-0.5">
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
      <div className="flex items-center gap-1 mt-0.5">
        <span className="text-[10px] text-text-secondary min-w-[24px]">Pos</span>
        <input
          type="number"
          className="form-input flex-1 min-w-0"
          value={ch.position}
          step={ch.vPerDiv}
          onChange={(e) =>
            onPatchChannel(idx, { position: parseFloat(e.target.value) || 0 })
          }
        />
        <span className="text-[10px] text-text-secondary min-w-[12px]">{unit}</span>
      </div>
    </div>
  );
}
