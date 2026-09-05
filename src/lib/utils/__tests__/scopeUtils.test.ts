import { describe, expect, it } from 'vitest';
import { formatVPerDiv } from '../../../types';

// computeMeasurements / computeAutoSetConfig / snapVPerDivUp 已后端化:
// 逻辑与回归测试由 Rust 侧 dsp_measure (stats/period/autoset) 承接。

describe('formatVPerDiv', () => {
  it('表内常规档位保持原样输出', () => {
    expect(formatVPerDiv(1)).toBe('1V/div');
    expect(formatVPerDiv(0.5)).toBe('500mV/div');
    expect(formatVPerDiv(1000)).toBe('1kV/div');
  });

  it('小数值使用 µ/n 前缀, 不再显示 0µ', () => {
    expect(formatVPerDiv(2e-5)).toBe('20µV/div');
    expect(formatVPerDiv(2e-8)).toBe('20nV/div');
    // 小于最小前缀时夹逼到 n
    expect(formatVPerDiv(2e-10)).toBe('0.2nV/div');
  });

  it('自定义单位与空单位', () => {
    expect(formatVPerDiv(5, 'A')).toBe('5A/div');
    expect(formatVPerDiv(5, '')).toBe('5/div');
  });

  it('非法值原样输出', () => {
    expect(formatVPerDiv(0)).toBe('0V/div');
    expect(formatVPerDiv(NaN)).toBe('NaNV/div');
  });
});
