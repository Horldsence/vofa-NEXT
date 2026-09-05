import { afterAll, bench, describe, expect } from 'vitest';
import { decodeWaveformWindow } from '../waveformProtocol';
import { normalizeWaveformWindow } from '../../buffers/dataBuffer';
import { applyCoupling } from '../../utils/scopeUtils';
import type { WaveformWindowPayload } from '../../../types';

// 固定 WWB1 v2 载荷，构造不计入测量。仅量化 JS CPU，不代表 WebView 绘制。
function fixture(channels: number, points: number) {
  const buffer = new ArrayBuffer(80 + points * (8 + channels * 4));
  const view = new DataView(buffer);
  view.setUint32(0, 0x31425757, true);
  view.setUint16(4, 2, true);
  view.setUint32(48, points, true);
  view.setUint32(52, channels, true);
  view.setUint32(56, channels, true);
  const timestamps = new Float64Array(buffer, 80, points);
  for (let i = 0; i < points; i++) timestamps[i] = (i - points + 1) * 0.1;
  for (let ch = 0; ch < channels; ch++) {
    const values = new Float32Array(buffer, 80 + points * 8 + ch * points * 4, points);
    for (let i = 0; i < points; i++) values[i] = i % 997 === 0 ? NaN : Math.sin(i * 0.017 + ch);
  }
  return buffer;
}

for (const [channels, points] of [[4, 2_000], [4, 12_000], [16, 12_000]]) {
  const binary = fixture(channels, points);
  const decoded = decodeWaveformWindow(binary);
  const json = JSON.stringify({ ...decoded, timestamps: Array.from(decoded.timestamps), channels: decoded.channels.map((c) => Array.from(c)) });
  describe(`${channels}ch_${points}points`, () => {
    let result: unknown;
    afterAll(() => { expect(result).toBeDefined(); });
    bench('WWB1 decode', () => { result = decodeWaveformWindow(binary); });
    bench('JSON parse + normalize', () => { result = normalizeWaveformWindow(JSON.parse(json) as WaveformWindowPayload); });
    bench('all channels AC coupling (渲染用; 测量已后端化)', () => { result = decoded.channels.map((c) => applyCoupling(c, 'AC')); });
  });
}
