export interface NumericControlRange {
  min: number;
  max: number;
  step: number;
}

const MAX_DECIMALS = 10;

export function decimalPlaces(value: number): number {
  if (!Number.isFinite(value)) return 0;
  const text = Math.abs(value).toString().toLowerCase();
  const [coefficient, exponentText] = text.split('e');
  const exponent = Number(exponentText ?? 0);
  const fractionLength = coefficient?.split('.')[1]?.length ?? 0;
  return Math.min(MAX_DECIMALS, Math.max(0, fractionLength - exponent));
}

export function numericPrecision(min: number, step: number): number {
  return Math.min(MAX_DECIMALS, Math.max(decimalPlaces(min), decimalPlaces(step)));
}

export function snapControlValue(
  value: number,
  { min, max, step }: NumericControlRange,
): number {
  if (![value, min, max, step].every(Number.isFinite) || min >= max || step <= 0) {
    return Number.isFinite(value) ? value : 0;
  }
  const precision = Math.min(
    MAX_DECIMALS,
    Math.max(numericPrecision(min, step), decimalPlaces(value), decimalPlaces(max)),
  );
  const factor = 10 ** precision;
  const scaledMin = Math.round(min * factor);
  const scaledMax = Math.round(max * factor);
  const scaledStep = Math.max(1, Math.round(step * factor));
  const scaledValue = Math.round(value * factor);
  const snapped = scaledMin + Math.round((scaledValue - scaledMin) / scaledStep) * scaledStep;
  const clamped = Math.max(scaledMin, Math.min(scaledMax, snapped));
  const rounded = clamped / factor;
  return Object.is(rounded, -0) ? 0 : rounded;
}

export function formatControlValue(value: number, min: number, step: number): string {
  if (!Number.isFinite(value)) return '—';
  const precision = numericPrecision(min, step);
  const rounded = value.toFixed(precision);
  if (precision === 0) return rounded;
  return rounded.replace(/\.?0+$/, '');
}

export function validateNumericRange(range: NumericControlRange): string | null {
  if (![range.min, range.max, range.step].every(Number.isFinite)) return 'finite';
  if (range.min >= range.max) return 'range';
  if (range.step <= 0) return 'step';
  return null;
}
