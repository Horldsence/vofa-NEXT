import { describe, expect, it } from 'vitest';
import {
  computeChecksum,
  crc8,
  crc16Modbus,
  crc16CCITT,
  crc32,
  sum8,
  xor8,
  lrc,
} from '../utils/checksum';

const CHECK_DATA = new TextEncoder().encode('123456789');

describe('checksum', () => {
  it('computes the CRC-8 check value (0xF4) for "123456789"', () => {
    expect(crc8(CHECK_DATA)).toEqual([0xf4]);
  });

  it('computes the CRC-16/Modbus check value (0x4B37) for "123456789"', () => {
    expect(crc16Modbus(CHECK_DATA)).toEqual([0x37, 0x4b]);
  });

  it('computes the CRC-16/CCITT-FALSE check value (0x29B1) for "123456789"', () => {
    expect(crc16CCITT(CHECK_DATA)).toEqual([0x29, 0xb1]);
  });

  it('computes the CRC-32 check value (0xCBF43926) for "123456789"', () => {
    expect(crc32(CHECK_DATA)).toEqual([0x26, 0x39, 0xf4, 0xcb]);
  });

  it('computes sum8 / xor8 / lrc over a byte array', () => {
    const bytes = new Uint8Array([0x01, 0x02, 0x03]);
    expect(sum8(bytes)).toEqual([0x06]);
    expect(xor8(bytes)).toEqual([0x00]);
    expect(lrc(bytes)).toEqual([0xfa]);
  });

  it('dispatches through computeChecksum and returns [] for "none"', () => {
    expect(computeChecksum(CHECK_DATA, 'none')).toEqual([]);
    expect(computeChecksum(CHECK_DATA, 'crc16Modbus')).toEqual([0x37, 0x4b]);
    expect(computeChecksum(CHECK_DATA, 'sum8')).toEqual([0xdd]);
  });
});
