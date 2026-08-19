import { describe, expect, it } from 'vitest';
import {
  justfloatSchema,
  firewaterSchema,
  rawDataSchema,
  slcanSchema,
  candleLightSchema,
  logicDecodeSchema,
  schemaFromProtocolConfig,
  schemaPortNames,
  protocolPortNames,
} from '../protocolSchema';
import type { DecoderBlock } from '../../../types';

describe('protocolSchema 预设工厂', () => {
  it('justfloatSchema 手动 n 通道: n×field{float32LE, ch{i}} + tail', () => {
    const s = justfloatSchema(3);
    expect(s.preset).toBe('justFloat');
    expect(s.legacyConfig).toEqual({ kind: 'JustFloat', channels: 3 });
    expect(s.decode).toHaveLength(4);
    for (let i = 0; i < 3; i++) {
      expect(s.decode[i]).toMatchObject({ type: 'field', fieldType: 'float32LE', portName: `ch${i}` });
    }
    expect(s.decode[3]).toMatchObject({ type: 'tail', hex: '00 00 80 7F' });
  });

  it('justfloatSchema 自动 (null) 也生成默认 4 端口', () => {
    const s = justfloatSchema(null);
    expect(s.legacyConfig).toEqual({ kind: 'JustFloat', channels: null });
    expect(schemaPortNames(s.decode)).toEqual(['ch0', 'ch1', 'ch2', 'ch3']);
  });

  it('firewaterSchema: csv 块端口 ch0..chN', () => {
    const s = firewaterSchema(2);
    expect(s.preset).toBe('fireWater');
    expect(s.legacyConfig).toEqual({ kind: 'FireWater', channels: 2 });
    expect(s.decode).toEqual([{ type: 'csv', separator: ',', ports: ['ch0', 'ch1'] }]);
    // 自动模式同样默认 4 端口
    expect(schemaPortNames(firewaterSchema(null).decode)).toEqual(['ch0', 'ch1', 'ch2', 'ch3']);
  });

  it('rawData/slcan/candleLight: 无 decode 块, legacyConfig 保留', () => {
    expect(rawDataSchema()).toEqual({ preset: 'rawData', legacyConfig: { kind: 'RawData' }, decode: [] });
    expect(slcanSchema()).toEqual({ preset: 'slcan', legacyConfig: { kind: 'Slcan' }, decode: [] });
    expect(candleLightSchema()).toEqual({ preset: 'candleLight', legacyConfig: { kind: 'CandleLight' }, decode: [] });
  });

  it('logicDecodeSchema: samples 块携带解码器配置', () => {
    const decoder = { kind: 'Uart' as const, params: { baud_rate: 115200, data_bits: 8, parity: 'none' as const, stop_bits: 'one' as const, channel: 0 } };
    const s = logicDecodeSchema(decoder);
    expect(s.preset).toBe('logicDecode');
    expect(s.legacyConfig).toEqual({ kind: 'LogicDecode', decoder });
    expect(s.decode).toEqual([{ type: 'samples', decoder }]);
  });

  it('schemaFromProtocolConfig: 各 kind 映射到对应预设', () => {
    expect(schemaFromProtocolConfig({ kind: 'JustFloat', channels: 2 }).preset).toBe('justFloat');
    expect(schemaFromProtocolConfig({ kind: 'FireWater', channels: null }).preset).toBe('fireWater');
    expect(schemaFromProtocolConfig({ kind: 'RawData' }).preset).toBe('rawData');
    expect(schemaFromProtocolConfig({ kind: 'Slcan' }).preset).toBe('slcan');
    expect(schemaFromProtocolConfig({ kind: 'CandleLight' }).preset).toBe('candleLight');
    expect(
      schemaFromProtocolConfig({ kind: 'LogicDecode', decoder: { kind: 'I2c', params: { sda_channel: 0, scl_channel: 1 } } }).preset
    ).toBe('logicDecode');
  });
});

describe('schemaPortNames (custom 端口派生, 与 Rust port_names 对齐)', () => {
  it('field/bitfield/csv.ports/asciiField 按块序去重; header/length/id/checksum/tail/samples 不占端口', () => {
    // 与 Rust test_port_names_derivation 相同的块序列
    const decode: DecoderBlock[] = [
      { id: 'h', type: 'header', hex: 'AA' },
      { id: 'f0', type: 'field', fieldType: 'uint8', portName: 'v0' },
      { id: 'b0', type: 'bitfield', byteOffset: 1, bitOffset: 0, bitLength: 4, isSigned: false, portName: 'flags' },
      { type: 'csv', separator: ',', ports: ['c0', 'c1'] },
      { type: 'asciiField', portName: 'hex_id', base: 'hex', digits: 2 },
    ];
    expect(schemaPortNames(decode)).toEqual(['v0', 'flags', 'c0', 'c1', 'hex_id']);
  });

  it('重复端口名去重且保持首次出现顺序', () => {
    const decode: DecoderBlock[] = [
      { id: 'f0', type: 'field', fieldType: 'uint8', portName: 'a' },
      { id: 'f1', type: 'field', fieldType: 'uint8', portName: 'b' },
      { id: 'f2', type: 'field', fieldType: 'uint8', portName: 'a' },
      { type: 'csv', separator: ',', ports: ['b', 'c'] },
    ];
    expect(schemaPortNames(decode)).toEqual(['a', 'b', 'c']);
  });
});

describe('protocolPortNames', () => {
  it('预设路径: ch0..chN (channels 来自节点 data, 与现有行为一致)', () => {
    const ports = protocolPortNames(
      { config: { kind: 'JustFloat', channels: 2 }, channels: 2, schema: justfloatSchema(2) },
      null
    );
    expect(ports).toEqual(['ch0', 'ch1']);
  });

  it('预设路径缺 schema (旧数据): 按 config/channels 回退 ch0..chN', () => {
    expect(
      protocolPortNames({ config: { kind: 'JustFloat', channels: null }, channels: 4 }, null)
    ).toEqual(['ch0', 'ch1', 'ch2', 'ch3']);
  });

  it('custom schema: 命名端口由 decode 块派生', () => {
    const ports = protocolPortNames(
      {
        config: { kind: 'JustFloat', channels: 2 },
        channels: 2,
        schema: {
          preset: 'custom',
          legacyConfig: null,
          decode: [
            { id: 'f0', type: 'field', fieldType: 'float32LE', portName: 'speed' },
            { id: 'f1', type: 'field', fieldType: 'float32LE', portName: 'temp' },
          ],
        },
      },
      null
    );
    expect(ports).toEqual(['speed', 'temp']);
  });
});
