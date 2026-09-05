import { describe, expect, it } from 'vitest';
import {
  computeNiceRange,
  decimalsForStep,
  formatTick,
  niceStep,
  tickValues,
} from '../valueRange';

describe('niceStep', () => {
  it('picks 1-2-5 mantissa across decades', () => {
    expect(niceStep(4, 4)).toBe(1); // raw 1 → 1
    expect(niceStep(10, 4)).toBe(5); // raw 2.5 → 5
    expect(niceStep(8, 4)).toBe(2); // raw 2 → 2
    expect(niceStep(0.3, 3)).toBe(0.1); // raw 0.1
    expect(niceStep(700, 7)).toBe(100); // raw 100
  });

  it('falls back to 1 on degenerate input', () => {
    expect(niceStep(0, 5)).toBe(1);
    expect(niceStep(-1, 5)).toBe(1);
    expect(niceStep(10, 0)).toBe(1);
    expect(niceStep(NaN, 5)).toBe(1);
  });
});

describe('computeNiceRange', () => {
  it('snaps bounds outward to step multiples so ticks land on bounds', () => {
    // 观测 [3, 47]: span 44, 4 格 raw 11 → step 20 → 边界外扩到 [0, 60]
    expect(computeNiceRange(3, 47, 5)).toEqual({ min: 0, max: 60 });
  });

  it('keeps bounds symmetric around zero', () => {
    const range = computeNiceRange(-4.2, 4.2, 5);
    expect(range.min).toBeLessThanOrEqual(-4.2);
    expect(range.max).toBeGreaterThanOrEqual(4.2);
    expect(range.max).toBe(-range.min);
  });

  it('expands flat signals to ±1 around the rounded center', () => {
    expect(computeNiceRange(5, 5, 5)).toEqual({ min: 4, max: 6 });
    expect(computeNiceRange(-2, -2, 5)).toEqual({ min: -3, max: -1 });
  });

  it('guards non-finite and inverted input', () => {
    expect(computeNiceRange(NaN, 5, 5)).toEqual({ min: 0, max: 1 });
    expect(computeNiceRange(5, -5, 5)).toEqual(computeNiceRange(-5, 5, 5));
  });
});

describe('tickValues', () => {
  it('returns majorTicks labels inclusive of both bounds', () => {
    expect(tickValues({ min: 0, max: 100 }, 5)).toEqual([0, 25, 50, 75, 100]);
  });

  it('clamps to at least 2 ticks and handles degenerate range', () => {
    expect(tickValues({ min: 1, max: 2 }, 0)).toEqual([1, 2]);
    expect(tickValues({ min: 3, max: 3 }, 5)).toEqual([3]);
  });
});

describe('decimalsForStep / formatTick', () => {
  it('derives exact decimals from step magnitude', () => {
    expect(decimalsForStep(1)).toBe(0);
    expect(decimalsForStep(2)).toBe(0);
    expect(decimalsForStep(0.5)).toBe(1);
    expect(decimalsForStep(0.25)).toBe(2);
    expect(decimalsForStep(0.005)).toBe(3);
    expect(decimalsForStep(100)).toBe(0);
  });

  it('formats ticks with auto precision from range spacing', () => {
    expect(formatTick(25, 'auto', { min: 0, max: 100 }, 5)).toBe('25');
    expect(formatTick(0.25, 'auto', { min: 0, max: 1 }, 5)).toBe('0.25');
    expect(formatTick(0.5, 1, { min: 0, max: 1 }, 5)).toBe('0.5');
  });
});
