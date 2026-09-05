import { describe, expect, it } from 'vitest';
import type { WidgetConfig } from '../../../types';
import { formatControlValue, snapControlValue, validateNumericRange } from '../numericControl';
import { widgetInputValue } from '../widgetDefaults';
import { normalizeWidgetConfig } from '../widgetNormalize';

const legacy = (kind: WidgetConfig['kind'], params: Record<string, unknown>) =>
  ({ kind, params } as unknown as WidgetConfig);

describe('numeric controls', () => {
  it('snaps relative to min without binary floating point residue', () => {
    expect(snapControlValue(0.35, { min: 0.1, max: 1, step: 0.1 })).toBe(0.4);
    expect(snapControlValue(10, { min: -1, max: 1, step: 0.25 })).toBe(1);
    expect(snapControlValue(-10, { min: -1, max: 1, step: 0.25 })).toBe(-1);
  });

  it('formats only meaningful decimals', () => {
    expect(formatControlValue(50, 0, 1)).toBe('50');
    expect(formatControlValue(1.5, 0, 0.1)).toBe('1.5');
    expect(formatControlValue(0.125, 0, 0.125)).toBe('0.125');
  });

  it('validates finite ordered ranges and positive steps', () => {
    expect(validateNumericRange({ min: 0, max: 1, step: 0.1 })).toBeNull();
    expect(validateNumericRange({ min: 1, max: 1, step: 1 })).toBe('range');
    expect(validateNumericRange({ min: 0, max: 1, step: 0 })).toBe('step');
  });
});

describe('input widget migration', () => {
  it('migrates numeric default and disables targetless legacy binding', () => {
    const widget = normalizeWidgetConfig(legacy('Knob', {
      id: 'knob-1', min: 0, max: 10, step: 0.5, default: 2.5,
      binding: { mode: 'Auto', params: { channel: 2 } },
    }));
    expect(widget).toMatchObject({
      kind: 'Knob',
      params: { label: 'Knob', value: 2.5, binding: { mode: 'None' } },
    });
  });

  it('migrates radio tuples and index selection to stable option ids', () => {
    const widget = normalizeWidgetConfig(legacy('Radio', {
      id: 'radio-1', label: 'Mode', options: [['Low', 10], ['High', 20]], default: 1,
      binding: { mode: 'None' },
    }));
    expect(widget.kind).toBe('Radio');
    if (widget.kind !== 'Radio') return;
    expect(widget.params.options).toEqual([
      { id: 'radio-1-option-1', label: 'Low', value: 10 },
      { id: 'radio-1-option-2', label: 'High', value: 20 },
    ]);
    expect(widget.params.selectedId).toBe('radio-1-option-2');
    expect(widgetInputValue(widget)).toBe(20);
  });

  it('migrates a legacy checkbox and preserves its empty value', () => {
    const widget = normalizeWidgetConfig(legacy('Checkbox', {
      id: 'check-1', label: 'Enable', checked_value: 7, unchecked_value: -1, default: false,
      binding: { mode: 'None' },
    }));
    expect(widget.kind).toBe('Checkbox');
    if (widget.kind !== 'Checkbox') return;
    expect(widget.params.options[0]).toEqual({ id: 'check-1-option-1', label: 'Option 1', value: 7 });
    expect(widget.params.selectedIds).toEqual([]);
    expect(widgetInputValue(widget)).toBe(-1);
  });

  it('sums selected checkbox option values', () => {
    const widget = normalizeWidgetConfig(legacy('Checkbox', {
      id: 'check-2', label: 'Flags',
      options: [
        { id: 'a', label: 'A', value: 1 },
        { id: 'b', label: 'B', value: 2 },
        { id: 'c', label: 'C', value: 4 },
      ],
      selectedIds: ['a', 'c'], binding: { mode: 'None' },
    }));
    expect(widgetInputValue(widget)).toBe(5);
  });
});

describe('display range migration (Gauge / Progress)', () => {
  it('migrates legacy gauge top-level min/max into a manual display range', () => {
    const widget = normalizeWidgetConfig(legacy('Gauge', {
      id: 'g1', label: 'G', min: 0, max: 200, unit: 'V', channel: 1,
    }));
    expect(widget).toMatchObject({
      kind: 'Gauge',
      params: {
        unit: 'V', channel: 1,
        range: { mode: 'manual', min: 0, max: 200, windowSec: 10, majorTicks: 5, precision: 'auto' },
      },
    });
    // 归一化幂等
    expect(normalizeWidgetConfig(widget)).toEqual(widget);
  });

  it('repairs invalid progress range fields and drops unknown shapes', () => {
    const widget = normalizeWidgetConfig(legacy('Progress', {
      id: 'p1',
      range: { mode: 'auto', min: 5, max: 2, windowSec: 0, majorTicks: 99, precision: 9 },
      orientation: 'diagonal', showValue: false, color: 'red',
    }));
    expect(widget.kind).toBe('Progress');
    if (widget.kind !== 'Progress') return;
    expect(widget.params.range).toEqual({
      mode: 'auto', min: 5, max: 105, windowSec: 10, majorTicks: 11, precision: 6,
    });
    expect(widget.params.orientation).toBe('horizontal');
    expect(widget.params.showValue).toBe(false);
    expect(widget.params.color).toBe('');
  });
});

