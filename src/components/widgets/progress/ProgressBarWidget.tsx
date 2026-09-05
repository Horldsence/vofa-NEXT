import { memo } from 'react';
import type { WidgetConfig } from '../../../types';
import { useNumericInput } from '../../../lib/hooks/useNumericPort';
import { useAutoRange } from '../../../lib/hooks/useAutoRange';
import { formatTick, tickValues } from '../../../lib/utils/valueRange';
import { NumericPortStatus } from '../../displays/common/NumericPortStatus';

interface ProgressProps {
  widget: Extract<WidgetConfig, { kind: 'Progress' }>;
}

/// 进度条控件 — 横向/纵向条形显示单通道实时值占比
/// 数据源: edge 连线优先, 回退 channel 参数; 量程手动或滑动窗口自适应。
/// 刻度: 主刻度数量/精度由配置决定 (纵向空间紧凑, 仅画刻度线不画标签)。
export const ProgressBar = memo(function Progress({ widget }: ProgressProps) {
  const params = widget.params;
  const { range: rangeConfig, unit, channel, orientation, showValue, color } = params;
  const input = useNumericInput(params.id, 'value', channel);
  const range = useAutoRange(params.id, input, rangeConfig);
  const value = input.latest?.value ?? range.min;
  const ratio = Math.max(0, Math.min(1, (value - range.min) / (range.max - range.min || 1)));
  const fillColor = color || 'var(--color-blue)';
  const ticks = tickValues(range, rangeConfig.majorTicks);
  const readout = input.latest
    ? formatTick(value, rangeConfig.precision, range, rangeConfig.majorTicks)
    : '—';

  if (orientation === 'vertical') {
    return (
      <div className="flex gap-2 w-full h-full min-h-[56px] items-stretch justify-center py-0.5">
        <div className="relative w-2.5 rounded-full bg-bg-input overflow-hidden flex flex-col justify-end">
          <div
            className="w-full rounded-full transition-[height] duration-150"
            style={{ height: `${ratio * 100}%`, backgroundColor: fillColor }}
          />
        </div>
        {/* 纵向刻度线 — 右侧细线, 无标签 (空间紧凑) */}
        <div className="relative w-2 h-full">
          {ticks.map((_, i) => (
            <span
              key={i}
              className="absolute right-0 h-px w-1.5 bg-text-secondary/60"
              style={{ bottom: `${(i / (ticks.length - 1 || 1)) * 100}%` }}
            />
          ))}
        </div>
        <div className="flex flex-col justify-between items-start py-0.5 min-w-0">
          {showValue && (
            <div className="font-mono text-sm font-semibold text-text-bright whitespace-nowrap">
              {readout}
              {unit && <span className="ml-0.5 text-[9px] text-text-secondary font-normal">{unit}</span>}
            </div>
          )}
          <NumericPortStatus state={input} />
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-1 w-full">
      {showValue && (
        <div className="flex items-baseline justify-between gap-1">
          <span className="font-mono text-sm font-semibold text-text-bright">
            {readout}
            {unit && <span className="ml-1 text-[10px] text-text-secondary font-normal">{unit}</span>}
          </span>
        </div>
      )}
      <div className="relative h-2.5 rounded-full bg-bg-input overflow-hidden">
        <div
          className="h-full rounded-full transition-[width] duration-150"
          style={{ width: `${ratio * 100}%`, backgroundColor: fillColor }}
        />
      </div>
      {/* 主刻度标签 — 均分排布, 两端对齐避免溢出 */}
      <div className="relative h-3">
        {ticks.map((tickVal, i) => (
          <span
            key={i}
            className={`absolute top-0 text-[8px] leading-3 text-text-secondary font-mono whitespace-nowrap ${
              i === 0 ? 'left-0' : i === ticks.length - 1 ? 'right-0' : ''
            }`}
            style={i === 0 || i === ticks.length - 1 ? undefined : { left: `${(i / (ticks.length - 1)) * 100}%`, transform: 'translateX(-50%)' }}
          >
            {formatTick(tickVal, rangeConfig.precision, range, rangeConfig.majorTicks)}
          </span>
        ))}
      </div>
      <NumericPortStatus state={input} />
    </div>
  );
});
