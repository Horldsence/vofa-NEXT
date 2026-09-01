import { describe, expect, it } from 'vitest';
import type { WaveformWindowPayload } from '../../../types';
import { normalizeWaveformWindow } from '../dataBuffer';

describe('normalizeWaveformWindow', () => {
  it('restores JSON null gaps to NaN at the single IPC boundary', () => {
    const payload: WaveformWindowPayload = {
      seq: 1,
      timestamps: [-0.001, 0],
      channels: [[null, 2]],
      channel_count: 1,
      derived: { wave: { custom: { out: [3, null] } } },
      buffer_points: 2,
      buffer_capacity: 100,
      latest_timestamp_us: 10_000,
      raw_window_points: 2,
      sampling: 'raw',
    };

    const normalized = normalizeWaveformWindow(payload);

    expect(normalized.channels[0][0]).toBeNaN();
    expect(normalized.channels[0][1]).toBe(2);
    expect(normalized.derived.wave.custom.out[0]).toBe(3);
    expect(normalized.derived.wave.custom.out[1]).toBeNaN();
  });
});
