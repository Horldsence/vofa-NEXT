import { describe, expect, it } from 'vitest';
import { resolveInputArray } from './waveformSeries';

describe('resolveInputArray', () => {
  it('uses source handle as part of a derived series identity', () => {
    const derived = {
      wave: {
        custom: {
          'out-a': [1, 2],
          'out-b': [10, 20],
        },
      },
    };

    expect(resolveInputArray(
      { kind: 'derived', sourceId: 'custom', sourceHandle: 'out-a' },
      'wave',
      2,
      [],
      derived,
    )).toEqual([1, 2]);
    expect(resolveInputArray(
      { kind: 'derived', sourceId: 'custom', sourceHandle: 'out-b' },
      'wave',
      2,
      [],
      derived,
    )).toEqual([10, 20]);
  });
});
