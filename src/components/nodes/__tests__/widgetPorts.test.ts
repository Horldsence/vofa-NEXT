import { describe, expect, it } from 'vitest';
import { getWidgetPorts } from '../WidgetPorts';
import type { StrOp, WidgetConfig } from '../../../types';
import { STR_OP_PORTS } from '../../../types';
import { createWidget } from '../../../lib/utils/widgetDefaults';

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

/// Str 控件工厂 (默认 pos/len/size, 端口仅由 op 决定)
function strWidget(op: StrOp): WidgetConfig {
  return { kind: 'Str', params: { id: 's1', label: 'Str', op, pos: 1, len: 0, size: 0 } };
}

const STR_OPS: StrOp[] = [
  'len', 'find', 'contains', 'left', 'right', 'mid', 'concat',
  'insert', 'delete', 'replace', 'upper', 'lower', 'trim', 'reverse',
  // 转换算子 (数值 ↔ 文本)
  'format', 'parse', 'encode_hex',
];

describe('getWidgetPorts - Str 字符串操作', () => {
  it('STR_OP_PORTS 覆盖全部 17 个 op', () => {
    expect(Object.keys(STR_OP_PORTS).sort()).toEqual([...STR_OPS].sort());
  });

  it.each(STR_OPS)('op=%s 的输入/输出端口与 STR_OP_PORTS 一致', (op) => {
    const meta = STR_OP_PORTS[op];
    const { inputs, outputs } = getWidgetPorts(strWidget(op));
    expect(inputs).toEqual(meta.inputs);
    expect(outputs).toEqual([{ id: 'result', label: 'result', domain: meta.outputDomain }]);
  });

  it('输出域覆盖数值与字符串两类 (len → time, concat → string)', () => {
    expect(getWidgetPorts(strWidget('len')).outputs[0].domain).toBe('time');
    expect(getWidgetPorts(strWidget('find')).outputs[0].domain).toBe('time');
    expect(getWidgetPorts(strWidget('contains')).outputs[0].domain).toBe('time');
    expect(getWidgetPorts(strWidget('concat')).outputs[0].domain).toBe('string');
    expect(getWidgetPorts(strWidget('upper')).outputs[0].domain).toBe('string');
  });

  it.each(STR_OPS)('op=%s 的内联数值端口均为输入端口表中的 time 域端口', (op) => {
    const meta = STR_OP_PORTS[op];
    const timePortIds = meta.inputs.filter((p) => p.domain === 'time').map((p) => p.id);
    for (const id of meta.inlineNumPorts) {
      expect(timePortIds).toContain(id);
    }
  });

  it('关键 op 的端口形状与后端端口表一致', () => {
    // mid: str (string) + pos/len (time) → result (string)
    expect(getWidgetPorts(strWidget('mid'))).toEqual({
      inputs: [
        { id: 'str', label: 'str', domain: 'string' },
        { id: 'pos', label: 'pos', domain: 'time' },
        { id: 'len', label: 'len', domain: 'time' },
      ],
      outputs: [{ id: 'result', label: 'result', domain: 'string' }],
    });
    // replace: str1/str2 (string) + pos/len (time) → result (string)
    expect(getWidgetPorts(strWidget('replace')).inputs.map((p) => p.id)).toEqual(['str1', 'str2', 'pos', 'len']);
    // insert: str1/str2 (string) + pos (time)
    expect(getWidgetPorts(strWidget('insert')).inputs.map((p) => p.id)).toEqual(['str1', 'str2', 'pos']);
    // left: str (string) + size (time)
    expect(getWidgetPorts(strWidget('left')).inputs.map((p) => p.id)).toEqual(['str', 'size']);
  });
});

/// TextInput 控件 (文本输入源, 无输入端口)
const TEXT_INPUT_WIDGET: WidgetConfig = {
  kind: 'TextInput',
  params: { id: 'ti-1', label: 'TextInput', text: 'hello', placeholder: '' },
};

describe('getWidgetPorts - TextInput 文本输入', () => {
  it('无输入端口, 输出恰为 str (string 域)', () => {
    const { inputs, outputs } = getWidgetPorts(TEXT_INPUT_WIDGET);
    expect(inputs).toEqual([]);
    expect(outputs.map((p) => ({ id: p.id, domain: p.domain }))).toEqual([
      { id: 'str', domain: 'string' },
    ]);
  });
});

describe('getWidgetPorts - Model3D 姿态输入格式', () => {
  it('欧拉角模式暴露 roll/pitch/yaw 端口', () => {
    const widget = createWidget('Model3D');
    expect(getWidgetPorts(widget).inputs.map((p) => p.id)).toEqual([
      'x', 'y', 'z', 'roll', 'pitch', 'yaw',
    ]);
  });

  it('四元数模式切换为 q0/q1/q2/q3 端口', () => {
    const widget = createWidget('Model3D');
    if (widget.kind !== 'Model3D') throw new Error('expected Model3D widget');
    widget.params.attitudeInputMode = 'quaternion';
    expect(getWidgetPorts(widget).inputs.map((p) => p.id)).toEqual([
      'x', 'y', 'z', 'q0', 'q1', 'q2', 'q3',
    ]);
  });
});
