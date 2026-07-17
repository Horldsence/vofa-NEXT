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
/// - V/div 大旋钮 (72px) + 下拉  (sharedY=true 时隐藏, 提示在全部 Tab 设置)
/// - Position 输入 (sharedY=true 时隐藏)
export function ChannelTabContent({
  idx,
  ch,
  yUnit,
  sharedY,
  onPatchChannel,
  renderStepSelect,
  lang,
}: {
  idx: number;
  ch: ChannelAxisConfig;
  yUnit: string;
  sharedY: boolean;
  onPatchChannel: (idx: number, p: Partial<ChannelAxisConfig>) => void;
  renderStepSelect: RenderStepSelect;
  lang: Lang;
}) {
  // Y 轴单位 (不一定是电压, 如 'A'/'°C'/'', 默认 'V')
  const unit = yUnit ?? 'V';
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

      {sharedY ? (
        // 共用 Y 模式: V/div/Position 在全部 Tab 统一设置, 这里只显示提示
        <div className="scope-section">
          <div className="scope-shared-y-hint">
            {t(lang, 'sharedYHint')}
          </div>
        </div>
      ) : (
        <>
          <div className="scope-section">
            <div className="scope-section-title">{unit}/div</div>
            <div className="scope-knob-row standalone">
              <StepKnob
                value={ch.vPerDiv}
                steps={V_PER_DIV}
                onChange={(v) => onPatchChannel(idx, { vPerDiv: v })}
                defaultValue={1}
                formatValue={(v) => formatVPerDiv(v, unit)}
                size={72}
                disabled={!ch.show}
              />
            </div>
            <div className="scope-knob-row">
              {renderStepSelect(
                V_PER_DIV,
                ch.vPerDiv,
                (v) => onPatchChannel(idx, { vPerDiv: v }),
                (v) => formatVPerDiv(v, unit)
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
              <span className="scope-unit">{unit}</span>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
