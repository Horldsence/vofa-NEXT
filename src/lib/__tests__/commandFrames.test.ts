import { describe, expect, it } from 'vitest';
import {
  normalizeCommandConfig,
  getCommandFrames,
  commandInputPortNames,
  addCommandFrame,
  removeCommandFrame,
  updateCommandFrame,
  makeEmptyFrame,
  computeFrameBytes,
} from '../utils/commandFrames';
import type { CommandConfig, LegacyCommandConfig } from '../../types';

/// 旧版 (单帧) 配置样例
const LEGACY: LegacyCommandConfig = {
  id: 'cmd-1',
  label: 'Cmd',
  blocks: [
    { id: 'b1', type: 'const_hex', hex: 'AA 01' },
    { id: 'b2', type: 'var_ref', portName: 'speed', fieldType: 'uint16LE' },
  ],
  appendNewline: true,
  loopbackEnabled: true,
  sendMode: 'timer',
  timerMs: 250,
  loopbackHistory: [],
};

describe('normalizeCommandConfig (旧版单帧 → frames)', () => {
  it('旧配置包装为单帧, 帧继承 blocks/appendNewline/sendMode/timerMs', () => {
    const cfg = normalizeCommandConfig(LEGACY);
    expect(cfg.frames).toHaveLength(1);
    expect(cfg.frames[0].blocks).toEqual(LEGACY.blocks);
    expect(cfg.frames[0].appendNewline).toBe(true);
    expect(cfg.frames[0].sendMode).toBe('timer');
    expect(cfg.frames[0].timerMs).toBe(250);
    // 发送器级字段保留
    expect(cfg.loopbackEnabled).toBe(true);
    expect(cfg.loopbackHistory).toEqual([]);
    // 顶层不再携带旧字段
    expect('blocks' in cfg).toBe(false);
    expect('sendMode' in cfg).toBe(false);
  });

  it('已是多帧配置时原样保留 (幂等)', () => {
    const once = normalizeCommandConfig(LEGACY);
    const twice = normalizeCommandConfig(once);
    expect(twice).toEqual(once);
  });

  it('frames 为空数组时补一个空帧', () => {
    const cfg = normalizeCommandConfig({
      id: 'c', label: 'C', frames: [], loopbackEnabled: false, loopbackHistory: [],
    });
    expect(cfg.frames).toHaveLength(1);
    expect(cfg.frames[0].blocks).toEqual([]);
  });

  it('getCommandFrames 对未归一化旧配置现场包装', () => {
    const frames = getCommandFrames(LEGACY);
    expect(frames).toHaveLength(1);
    expect(frames[0].blocks).toEqual(LEGACY.blocks);
  });
});

describe('commandInputPortNames (全帧 var_ref 并集)', () => {
  it('多帧端口名取并集, 去重且保序', () => {
    const cfg: CommandConfig = {
      id: 'cmd-2',
      label: 'C',
      frames: [
        {
          id: 'f1', label: 'F1', appendNewline: false, sendMode: 'manual', timerMs: 100,
          blocks: [
            { id: 'a', type: 'var_ref', portName: 'speed', fieldType: 'uint16LE' },
            { id: 'b', type: 'var_ref', portName: 'dir', fieldType: 'uint8' },
          ],
        },
        {
          id: 'f2', label: 'F2', appendNewline: false, sendMode: 'manual', timerMs: 100,
          blocks: [
            { id: 'c', type: 'var_ref', portName: 'speed', fieldType: 'uint16LE' }, // 重复
            { id: 'd', type: 'var_ref', portName: 'temp', fieldType: 'int16LE' },
            { id: 'e', type: 'const_hex', hex: 'AA' }, // 非 var_ref 不计
          ],
        },
      ],
      loopbackEnabled: false,
      loopbackHistory: [],
    };
    expect(commandInputPortNames(cfg)).toEqual(['speed', 'dir', 'temp']);
  });

  it('旧配置同样能取到端口名', () => {
    expect(commandInputPortNames(LEGACY)).toEqual(['speed']);
  });
});

describe('帧增删改', () => {
  const base = normalizeCommandConfig(LEGACY);

  it('addCommandFrame 追加一帧', () => {
    const f2 = makeEmptyFrame('cmd-1', 'F2');
    const cfg = addCommandFrame(base, f2);
    expect(cfg.frames).toHaveLength(2);
    expect(cfg.frames[1]).toEqual(f2);
    // 原配置不被修改
    expect(base.frames).toHaveLength(1);
  });

  it('removeCommandFrame 删除指定帧, 但至少保留一帧', () => {
    const f2 = makeEmptyFrame('cmd-1', 'F2');
    const cfg = addCommandFrame(base, f2);
    const removed = removeCommandFrame(cfg, f2.id);
    expect(removed.frames).toHaveLength(1);
    expect(removed.frames[0].id).toBe(base.frames[0].id);
    // 只剩一帧时删除无效
    const kept = removeCommandFrame(removed, removed.frames[0].id);
    expect(kept.frames).toHaveLength(1);
  });

  it('updateCommandFrame 只改指定帧 (改名/触发模式)', () => {
    const f2 = makeEmptyFrame('cmd-1', 'F2');
    const cfg = addCommandFrame(base, f2);
    const renamed = updateCommandFrame(cfg, f2.id, { label: '停機', sendMode: 'onChange' });
    expect(renamed.frames[1].label).toBe('停機');
    expect(renamed.frames[1].sendMode).toBe('onChange');
    // 其他帧不受影响
    expect(renamed.frames[0]).toEqual(cfg.frames[0]);
  });
});

describe('computeFrameBytes (按帧拼接字节流)', () => {
  it('const_hex + var_ref + checksum + appendNewline', () => {
    const frame = {
      id: 'f1', label: 'F1', appendNewline: true, sendMode: 'manual' as const, timerMs: 100,
      blocks: [
        { id: 'b1', type: 'const_hex' as const, hex: 'AA 01' },
        { id: 'b2', type: 'var_ref' as const, portName: 'speed', fieldType: 'uint16LE' as const },
        { id: 'b3', type: 'checksum' as const, checksum: 'sum8' as const },
      ],
    };
    const { bytes, error, perBlock } = computeFrameBytes(frame, { speed: 0x0203 });
    expect(error).toBeNull();
    // AA 01 + 03 02 (uint16LE) + sum8(AA+01+03+02=B0) + 0A
    expect(Array.from(bytes!)).toEqual([0xaa, 0x01, 0x03, 0x02, 0xb0, 0x0a]);
    expect(perBlock).toHaveLength(3);
  });

  it('var_ref 缺连入值时按 0 编码; 非法 HEX 产出错误', () => {
    const frame = {
      id: 'f1', label: 'F1', appendNewline: false, sendMode: 'manual' as const, timerMs: 100,
      blocks: [{ id: 'b1', type: 'var_ref' as const, portName: 'x', fieldType: 'uint8' as const }],
    };
    expect(Array.from(computeFrameBytes(frame, {}).bytes!)).toEqual([0]);

    const bad = {
      id: 'f2', label: 'F2', appendNewline: false, sendMode: 'manual' as const, timerMs: 100,
      blocks: [{ id: 'b1', type: 'const_hex' as const, hex: 'A' }],
    };
    const r = computeFrameBytes(bad, {});
    expect(r.bytes).toBeNull();
    expect(r.error).toBeTruthy();
  });
});
