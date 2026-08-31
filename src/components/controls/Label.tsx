import type { WidgetConfig } from '../../types';
import { WidgetCard } from '../ui/WidgetCard';
import { useNumericInput } from '../../lib/hooks/useNumericPort';
import { NumericPortStatus } from '../displays/common/NumericPortStatus';

interface LabelProps {
  widget: Extract<WidgetConfig, { kind: 'Label' }>;
  onRemove: () => void;
}

/// 标签控件 — 显示通道实时值或固定文本
export function Label({ widget, onRemove }: LabelProps) {
  const { text, channel } = widget.params;
  const input = useNumericInput(widget.params.id, 'value', channel);
  const display = input.latest ? `${text}: ${input.latest.value.toFixed(3)}` : text;

  return (
    <WidgetCard onRemove={onRemove}>
      <div className="text-xs text-text-secondary uppercase tracking-[0.3px]">{channel === null ? 'Label' : `CH${channel}`}</div>
      <div className="text-xl font-semibold text-text-bright font-mono text-center">{display}</div>
      <div className="text-center"><NumericPortStatus state={input} /></div>
    </WidgetCard>
  );
}
