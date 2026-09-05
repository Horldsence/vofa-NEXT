import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import { createWidget } from '../../../lib/utils/widgetDefaults';
import { normalizeWidgetConfig } from '../../../lib/utils/widgetNormalize';
import { ProgressBar } from '../progress/ProgressBarWidget';

describe('ProgressBar (display range contract)', () => {
  it('renders readout placeholder and min/max tick labels with no data', () => {
    const widget = normalizeWidgetConfig(createWidget('Progress'));
    if (widget.kind !== 'Progress') throw new Error('expected Progress');
    render(<ProgressBar widget={widget} />);

    // 无数据: 读数占位 —, 刻度首尾 = 配置量程边界
    expect(screen.getByText('—')).toBeDefined();
    expect(screen.getByText('0')).toBeDefined();
    expect(screen.getByText('100')).toBeDefined();
  });

  it('formats readout with configured manual range and precision', () => {
    const widget = normalizeWidgetConfig(createWidget('Progress'));
    if (widget.kind !== 'Progress') throw new Error('expected Progress');
    const configured = {
      ...widget,
      params: {
        ...widget.params,
        range: { ...widget.params.range, min: 0, max: 10, majorTicks: 5, precision: 1 },
      },
    };
    // 量程 0..10, 5 刻度 → 0/2.5/5/7.5/10
    render(<ProgressBar widget={configured} />);
    expect(screen.getByText('10.0')).toBeDefined();
    expect(screen.getByText('2.5')).toBeDefined();
  });
});
