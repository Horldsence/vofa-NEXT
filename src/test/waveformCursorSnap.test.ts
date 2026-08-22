//! pickNearestVisibleSlot 测试 — 光标吸附选择"距光标最近的可见曲线"
//!
//! 回归背景: 旧实现恒取第一条可见 slot, 导致吸附永远落在 ch0。
//! tauri / matchMedia / ResizeObserver 由 test/setup.ts 全局 mock。

import { describe, expect, it } from 'vitest';
import { pickNearestVisibleSlot } from '../components/displays/waveform/waveformChartHooks';
import { createDefaultScopeConfig } from '../types';
import type { SeriesSlot } from '../components/displays/waveform/waveformSeries';

/// 两个通道 slot: CH0 (cfgIdx=0), CH1 (cfgIdx=1)
const slots: SeriesSlot[] = [
  { input: { kind: 'channel', idx: 0 }, colorIdx: 0, isDerived: false, label: 'CH0', cfgIdx: 0 },
  { input: { kind: 'channel', idx: 1 }, colorIdx: 1, isDerived: false, label: 'CH1', cfgIdx: 1 },
];

/// dataX + CH0 恒 0 + CH1 恒 10
const dataX = [0, 1, 2];
const data = [dataX, [0, 0, 0], [10, 10, 10]];

const identity = (v: number) => v;

describe('pickNearestVisibleSlot', () => {
  it('鼠标 Y 靠近第二条曲线时吸附到第二条 (不再恒为 ch0)', () => {
    const cfg = createDefaultScopeConfig(2);
    expect(pickNearestVisibleSlot(slots, cfg, dataX, data, 1, 9, identity)).toBe(1);
  });

  it('鼠标 Y 靠近第一条曲线时吸附到第一条', () => {
    const cfg = createDefaultScopeConfig(2);
    expect(pickNearestVisibleSlot(slots, cfg, dataX, data, 1, 1, identity)).toBe(0);
  });

  it('valToPos 为反转像素映射 (真实图表 Y 轴向下) 时仍选最近通道', () => {
    const cfg = createDefaultScopeConfig(2);
    // y 值 0 -> 像素 100, y 值 10 -> 像素 0
    const valToPos = (v: number) => 100 - v * 10;
    expect(pickNearestVisibleSlot(slots, cfg, dataX, data, 1, 5, valToPos)).toBe(1);
    expect(pickNearestVisibleSlot(slots, cfg, dataX, data, 1, 95, valToPos)).toBe(0);
  });

  it('show=false 的通道被跳过', () => {
    const cfg = createDefaultScopeConfig(2);
    cfg.channels[1].show = false;
    expect(pickNearestVisibleSlot(slots, cfg, dataX, data, 1, 9, identity)).toBe(0);
  });

  it('插值为 NaN 的通道被跳过', () => {
    const cfg = createDefaultScopeConfig(2);
    const dataWithNaN = [dataX, [0, 0, 0], [NaN, NaN, NaN]];
    expect(pickNearestVisibleSlot(slots, cfg, dataX, dataWithNaN, 1, 9, identity)).toBe(0);
  });

  it('全部通道不可见时返回 -1', () => {
    const cfg = createDefaultScopeConfig(2);
    cfg.channels[0].show = false;
    cfg.channels[1].show = false;
    expect(pickNearestVisibleSlot(slots, cfg, dataX, data, 1, 9, identity)).toBe(-1);
  });

  it('无数据时返回 -1', () => {
    const cfg = createDefaultScopeConfig(2);
    expect(pickNearestVisibleSlot(slots, cfg, [], [[], [], []], 1, 9, identity)).toBe(-1);
  });

  it('距离相等时取先出现 (下标更小) 的通道', () => {
    const cfg = createDefaultScopeConfig(2);
    expect(pickNearestVisibleSlot(slots, cfg, dataX, data, 1, 5, identity)).toBe(0);
  });

  it('仅一条可见时吸附到该条', () => {
    const cfg = createDefaultScopeConfig(2);
    cfg.channels[0].show = false;
    expect(pickNearestVisibleSlot(slots, cfg, dataX, data, 1, 9, identity)).toBe(1);
  });
});
