import { memo } from 'react';
import type { WidgetConfig } from '../../../types';
import { useNumericInput } from '../../../lib/hooks/useNumericPort';
import { formatNumericValue, NumericPortStatus } from '../../displays/common/NumericPortStatus';

interface NumberDisplayProps {
  widget: Extract<WidgetConfig, { kind: 'NumberDisplay' }>;
}

/// 大数字显示 — 大字号展示单通道数值, 含单位与小数位
/// 数据源: edge 连线 (后端图输出) 优先, 否则回退到 channel 参数
export const NumberDisplay = memo(function NumberDisplay({ widget }: NumberDisplayProps) {
  const { unit, precision, channel } = widget.params;
  const input = useNumericInput(widget.params.id, 'value', channel);

  // 自适应字号: 值越长字号越小
  const text = formatNumericValue(input, precision);
  const fontSize = text.length > 10 ? 18 : text.length > 7 ? 24 : 32;

  return (
    <div>
      <div className="flex items-baseline justify-center gap-1 px-1 py-3 min-h-[56px]">
        <span className="font-mono font-bold text-text-bright tracking-[-0.5px] leading-none" style={{ fontSize }}>
          {text}
        </span>
        {unit && <span className="text-sm text-text-secondary font-normal">{unit}</span>}
      </div>
      <div className="text-center"><NumericPortStatus state={input} /></div>
      {channel === null && (
        <div className="text-[10px] text-text-secondary text-center opacity-60">未绑定通道</div>
      )}
    </div>
  );
});
