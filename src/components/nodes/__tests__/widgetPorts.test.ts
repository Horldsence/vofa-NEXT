import { describe, expect, it } from 'vitest';
import { getWidgetPorts } from '../WidgetNode';
import type { WidgetConfig } from '../../../types';

/// Command 控件 (两帧, var_ref 端口有重复)
const CMD_WIDGET: WidgetConfig = {
  kind: 'Command',
  params: {
    id: 'cmd-1',
    label: 'Cmd',
    frames: [
      {
        id: 'f1', label: 'F1', appendNewline: false, sendMode: 'manual', timerMs: 100,
        blocks: [
          { id: 'a', type: 'var_ref', portName: 'speed', fieldType: 'uint16LE' },
          { id: 'b', type: 'const_hex', hex: 'AA' },
        ],
      },
      {
        id: 'f2', label: 'F2', appendNewline: false, sendMode: 'timer', timerMs: 100,
        blocks: [
          { id: 'c', type: 'var_ref', portName: 'speed', fieldType: 'uint16LE' },
          { id: 'd', type: 'var_ref', portName: 'temp', fieldType: 'int16LE' },
        ],
      },
    ],
    loopbackEnabled: false,
    loopbackHistory: [],
  },
};

describe('getWidgetPorts - Command 多帧', () => {
  it('输入端口 = 所有帧 var_ref 块 portName 并集 (去重保序)', () => {
    const { inputs, outputs } = getWidgetPorts(CMD_WIDGET);
    expect(inputs.map((p) => p.id)).toEqual(['speed', 'temp']);
    // loopbackOut 字节出口保留
    expect(outputs.map((p) => p.id)).toEqual(['loopbackOut']);
    expect(outputs[0].domain).toBe('bytes');
  });

  it('旧版单帧配置 (blocks 在顶层) 也能派生端口', () => {
    const legacy = {
      kind: 'Command',
      params: {
        id: 'cmd-2',
        label: 'Cmd',
        blocks: [{ id: 'a', type: 'var_ref', portName: 'dir', fieldType: 'uint8' }],
        appendNewline: false,
        loopbackEnabled: false,
        sendMode: 'manual',
        timerMs: 100,
        loopbackHistory: [],
      },
    } as unknown as WidgetConfig;
    const { inputs } = getWidgetPorts(legacy);
    expect(inputs.map((p) => p.id)).toEqual(['dir']);
  });
});
