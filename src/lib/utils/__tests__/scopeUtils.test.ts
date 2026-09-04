import { describe, expect, it } from 'vitest';
import { computeAutoSetConfig, snapVPerDivUp, AUTOSET_WINDOW_SEC } from '../scopeUtils';
import { createDefaultScopeConfig, formatVPerDiv, TIME_BASES_SEC, type WaveformWindow } from '../../../types';

/// 构造 WaveformWindow 测试夹具 — ts 为相对毫秒, channels 与 channel_count 对齐
function makeWindow(channels: number[][], durMs = 1000): WaveformWindow {
  const n = channels[0]?.length ?? 0;
  const step = n > 1 ? durMs / (n - 1) : 0;
  return {
    seq: 1,
    timestamps: Float64Array.from(
      { length: n },
      (_, i) => -durMs + i * step,
    ),
    channels: channels.map((values) => Float32Array.from(values)),
    channel_count: channels.length,
    derived: {},
    buffer_points: n,
    buffer_capacity: Math.max(1, n),
    latest_timestamp_us: 0,
    raw_window_points: n,
    sampling: 'raw',
  };
}

describe('snapVPerDivUp', () => {
  it('向上取最近 1-2-5 档', () => {
    expect(snapVPerDivUp(1)).toBe(1);
    expect(snapVPerDivUp(1.4)).toBe(2);
    expect(snapVPerDivUp(3)).toBe(5);
    expect(snapVPerDivUp(7)).toBe(10);
    expect(snapVPerDivUp(0.13)).toBeCloseTo(0.2);
  });

  it('小信号可跨到 V_PER_DIV 表外 (µV/nV 档)', () => {
    expect(snapVPerDivUp(1.8e-5)).toBeCloseTo(2e-5);
    expect(snapVPerDivUp(3e-8)).toBeCloseTo(5e-8);
  });

  it('非法输入回退为 1', () => {
    expect(snapVPerDivUp(0)).toBe(1);
    expect(snapVPerDivUp(-3)).toBe(1);
    expect(snapVPerDivUp(NaN)).toBe(1);
    expect(snapVPerDivUp(Infinity)).toBe(1);
  });
});

describe('computeAutoSetConfig', () => {
  it('小信号 (Vpp < 1mV): 取到表外小档位, 波形不被压成直线', () => {
    // 100µV ~ 300µV 的小信号
    const ch = Array.from({ length: 100 }, (_, i) => 2e-4 + 1e-4 * Math.sin(i / 10));
    const cfg = createDefaultScopeConfig(1);
    cfg.sharedY = false;
    const next = computeAutoSetConfig(makeWindow([ch]), cfg, [0]);
    const vd = next.channels[0].vPerDiv;
    // vpp ≈ 2e-4 → target = 2e-4 / 5.6 ≈ 3.6e-5 → 向上取 5e-5
    expect(vd).toBeCloseTo(5e-5);
    // 信号跨度应占约 4 格 (而非旧逻辑的 0.2 格 ≈ 一条直线)
    const vpp = 2e-4;
    expect(vpp / vd).toBeGreaterThan(2);
    expect(vpp / vd).toBeLessThanOrEqual(8);
    // position 居中到信号中点
    expect(next.channels[0].position).toBeCloseTo(2e-4, 6);
  });

  it('大信号: 向上取档保证波形不顶出屏幕 (跨度 <= 8 格)', () => {
    const ch = Array.from({ length: 100 }, (_, i) => (i % 2 === 0 ? 0 : 39));
    const cfg = createDefaultScopeConfig(1);
    cfg.sharedY = false;
    const next = computeAutoSetConfig(makeWindow([ch]), cfg, [0]);
    const vd = next.channels[0].vPerDiv;
    // vpp=39 → target ≈ 6.96 → 向上取 10 (旧绝对差取档会选 5, 跨 7.8 格几乎顶满)
    expect(vd).toBe(10);
    expect(39 / vd).toBeLessThanOrEqual(8);
  });

  it('平直信号 (vpp=0): 保持当前 vPerDiv, 仅居中 position', () => {
    const ch = Array.from({ length: 50 }, () => 42);
    const cfg = createDefaultScopeConfig(1);
    cfg.sharedY = false;
    cfg.channels[0].vPerDiv = 5;
    const next = computeAutoSetConfig(makeWindow([ch]), cfg, [0]);
    expect(next.channels[0].vPerDiv).toBe(5);
    expect(next.channels[0].position).toBe(42);
  });

  it('sharedY 模式: 全局 min/max 取档写入 channels[0]', () => {
    const ch0 = Array.from({ length: 50 }, () => 1e-3);
    const ch1 = Array.from({ length: 50 }, (_, i) => -5e-4 + (i % 2) * 1e-3);
    const cfg = createDefaultScopeConfig(2);
    cfg.sharedY = true;
    const next = computeAutoSetConfig(makeWindow([ch0, ch1]), cfg, [0, 1]);
    // 全局 vpp = 1e-3 - (-5e-4) = 1.5e-3 → target ≈ 2.68e-4 → 向上取 5e-4
    expect(next.channels[0].vPerDiv).toBeCloseTo(5e-4);
    expect(next.channels[0].position).toBeCloseTo(2.5e-4, 6);
  });

  describe('金字塔全历史裁剪 (AUTOSET_WINDOW_SEC)', () => {
    /// 构造跨 totalSec 秒的长历史: 前 oldEndRatio 段为旧极值, 最近 recentSec 秒为小信号
    function longHistoryWindow(opts: {
      totalSec: number;
      recentSec: number;
      oldValue: number;
      recentVpp: number;
    }): WaveformWindow {
      const { totalSec, recentSec, oldValue, recentVpp } = opts;
      const rate = 100; // 100 Hz
      const n = totalSec * rate;
      const timestamps = Float64Array.from({ length: n }, (_, i) => (i / rate) * 1000);
      const recentStartSec = totalSec - recentSec;
      const ch = Float32Array.from({ length: n }, (_, i) => {
        const sec = i / rate;
        return sec < recentStartSec
          ? oldValue
          : (recentVpp / 2) * Math.sin((sec - recentStartSec) * Math.PI * 2);
      });
      return {
        seq: 1,
        timestamps,
        channels: [ch],
        channel_count: 1,
        derived: {},
        buffer_points: n,
        buffer_capacity: n,
        latest_timestamp_us: 0,
        raw_window_points: n,
        sampling: 'min_max',
      };
    }

    it('历史跨度超时基上限: 按最近窗口取档, 不再钳死在最大档', () => {
      // 120s 历史 → 全历史时基目标 12s/div (表内最大 5s/div, 旧逻辑永远钳死);
      // 裁剪后最近 10s → 目标 1s/div
      const cfg = createDefaultScopeConfig(1);
      const next = computeAutoSetConfig(
        longHistoryWindow({ totalSec: 120, recentSec: 10, oldValue: 50, recentVpp: 2 }),
        cfg,
        [0],
      );
      expect(next.timeBase).toBeLessThanOrEqual(1);
      expect(next.timeBase).toBe(TIME_BASES_SEC[TIME_BASES_SEC.indexOf(1)]);
    });

    it('深历史旧极值不撑大纵向档位 (独立 Y)', () => {
      // 最近 12s 内 Vpp≈2 (整个裁剪窗都被新信号覆盖), 旧历史是 ±50 平台 —
      // 档位应按 2 取 (≈0.5), 而非 100 (≈20)
      const cfg = createDefaultScopeConfig(1);
      cfg.sharedY = false;
      const next = computeAutoSetConfig(
        longHistoryWindow({ totalSec: 120, recentSec: 12, oldValue: 50, recentVpp: 2 }),
        cfg,
        [0],
      );
      expect(next.channels[0].vPerDiv).toBeCloseTo(0.5);
      expect(next.channels[0].position).toBeCloseTo(0, 5);
    });

    it('sharedY 同样只统计最近窗口', () => {
      const cfg = createDefaultScopeConfig(1);
      cfg.sharedY = true;
      const next = computeAutoSetConfig(
        longHistoryWindow({ totalSec: 120, recentSec: 12, oldValue: 50, recentVpp: 2 }),
        cfg,
        [0],
      );
      expect(next.channels[0].vPerDiv).toBeCloseTo(0.5);
    });

    it(`短历史 (≤${AUTOSET_WINDOW_SEC}s) 全窗口拟合, 行为不变`, () => {
      const ch = Array.from({ length: 100 }, (_, i) => (i % 2 === 0 ? 0 : 39));
      const cfg = createDefaultScopeConfig(1);
      cfg.sharedY = false;
      const next = computeAutoSetConfig(makeWindow([ch], 5000), cfg, [0]);
      // 与既有「大信号」用例同参数: vpp=39 → 向上取 10
      expect(next.channels[0].vPerDiv).toBe(10);
    });
  });
});

describe('formatVPerDiv', () => {
  it('表内常规档位保持原样输出', () => {
    expect(formatVPerDiv(1)).toBe('1V/div');
    expect(formatVPerDiv(0.5)).toBe('500mV/div');
    expect(formatVPerDiv(0.002)).toBe('2mV/div');
    expect(formatVPerDiv(5000)).toBe('5kV/div');
    expect(formatVPerDiv(10000)).toBe('10kV/div');
  });

  it('小数值使用 µ/n 前缀, 不再显示 0µ', () => {
    expect(formatVPerDiv(2e-5)).toBe('20µV/div');
    expect(formatVPerDiv(5e-5)).toBe('50µV/div');
    expect(formatVPerDiv(2e-10)).toBe('0.2nV/div');
  });

  it('自定义单位与空单位', () => {
    expect(formatVPerDiv(0.02, 'A')).toBe('20mA/div');
    expect(formatVPerDiv(2e-5, '')).toBe('20µ/div');
  });

  it('非法值原样输出', () => {
    expect(formatVPerDiv(0)).toBe('0V/div');
    expect(formatVPerDiv(NaN)).toBe('NaNV/div');
  });
});
