import { memo } from 'react';
import type { NumericPortState } from '../../../lib/data/numericTypes';

const STATUS_LABELS = {
  waiting: 'WAIT',
  live: '',
  disconnected: 'OFFLINE',
  channel_out_of_range: 'OUT OF RANGE',
  overrun: 'OVERRUN',
} as const;

export function numericValueOr(state: NumericPortState, fallback: number): number {
  return state.latest?.value ?? fallback;
}

export function formatNumericValue(state: NumericPortState, precision: number): string {
  return state.latest ? state.latest.value.toFixed(precision) : '—';
}

export const NumericPortStatus = memo(function NumericPortStatus({
  state,
}: {
  state: NumericPortState;
}) {
  const label = state.error ? 'ERROR' : STATUS_LABELS[state.status];
  if (!label || (!state.source && state.latest === null)) return null;
  return (
    <span
      className="text-[9px] text-text-secondary opacity-70 font-mono"
      title={state.error ?? state.status}
    >
      {label}
    </span>
  );
});
