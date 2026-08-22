import { describe, expect, it } from 'vitest';
import { widgetToNodeKind } from '../nodeDef';
import { createWidget } from '../createWidget';
import type { WidgetConfig } from '../../../types';

describe('widgetToNodeKind - Str', () => {
  it('StrConfig 的 pos/len/size 映射为 num, op 原样透传', () => {
    const widget: WidgetConfig = {
      kind: 'Str',
      params: { id: 's1', label: 'Str', op: 'mid', pos: 2, len: 3, size: 4 },
    };
    expect(widgetToNodeKind(widget)).toEqual({
      kind: 'Str',
      params: { op: 'mid', num: { pos: 2, len: 3, size: 4 } },
    });
  });

  it('JSON 序列化形状与后端 serde (tag=kind, content=params) 一致', () => {
    const widget: WidgetConfig = {
      kind: 'Str',
      params: { id: 's1', label: 'Str', op: 'mid', pos: 1, len: 0, size: 0 },
    };
    // 镜像后端: {"kind":"Str","params":{"op":"mid","num":{"pos":1.0,"len":0.0,"size":0.0}}}
    expect(JSON.parse(JSON.stringify(widgetToNodeKind(widget)))).toEqual({
      kind: 'Str',
      params: { op: 'mid', num: { pos: 1, len: 0, size: 0 } },
    });
  });

  it('createWidget 默认值: op=len, pos/len/size = 1/0/0', () => {
    const widget = createWidget('Str');
    expect(widget.kind).toBe('Str');
    expect(widgetToNodeKind(widget)).toEqual({
      kind: 'Str',
      params: { op: 'len', num: { pos: 1, len: 0, size: 0 } },
    });
  });
});

describe('widgetToNodeKind - TextInput', () => {
  it('TextInputConfig 的 text 映射为 NodeKind::TextInput.params.text', () => {
    const widget: WidgetConfig = {
      kind: 'TextInput',
      params: { id: 'ti-1', label: 'TextInput', text: 'hello', placeholder: '' },
    };
    expect(widgetToNodeKind(widget)).toEqual({
      kind: 'TextInput',
      params: { text: 'hello' },
    });
  });

  it('JSON 序列化形状与后端 serde (tag=kind, content=params) 一致', () => {
    const widget: WidgetConfig = {
      kind: 'TextInput',
      params: { id: 'ti-1', label: 'TextInput', text: 'hi', placeholder: '' },
    };
    // 镜像后端: {"kind":"TextInput","params":{"text":"hi"}}
    expect(JSON.parse(JSON.stringify(widgetToNodeKind(widget)))).toEqual({
      kind: 'TextInput',
      params: { text: 'hi' },
    });
  });

  it('createWidget 默认值: label=TextInput, text/placeholder 为空串', () => {
    const widget = createWidget('TextInput');
    expect(widget.kind).toBe('TextInput');
    expect(widget.params).toMatchObject({ label: 'TextInput', text: '', placeholder: '' });
    expect(widgetToNodeKind(widget)).toEqual({
      kind: 'TextInput',
      params: { text: '' },
    });
  });
});
