import { describe, expect, it } from 'vitest';
import { absoluteTimeRangeUs } from './waveformChartExport';

describe('absoluteTimeRangeUs', () => {
  it('使用框选时锚点，并规范化反向框选范围', () => {
    const anchored = absoluteTimeRangeUs(
      { startSec: -0.01, endSec: -0.02 },
      2_000_000,
    );

    expect(anchored).toEqual({ startUs: 1_980_000, endUs: 1_990_000 });
    // 后续实时窗口推进不会参与已经锚定的范围换算。
    expect(anchored).not.toEqual(absoluteTimeRangeUs(
      { startSec: -0.01, endSec: -0.02 },
      3_000_000,
    ));
  });
});
