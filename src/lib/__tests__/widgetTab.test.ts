import { describe, expect, it } from 'vitest';
import { widgetTabType, widgetToTab } from '../utils/widgetTab';
import type { WidgetConfig } from '../../types';

describe('widgetTab 控件 ↔ 窗口映射', () => {
  it('窗口型控件映射到对应 Tab 类型, Tab id 与控件 id 相同', () => {
    const cases: [WidgetConfig['kind'], string][] = [
      ['Waveform', 'waveform-extra'],
      ['PieChart', 'pie'],
      ['Image', 'image'],
      ['Model3D', 'model3d'],
      ['Spectrum', 'spectrum'],
      ['Command', 'command'],
      ['FrameDecoder', 'frame-decoder'],
      ['RawData', 'raw'],
    ];
    for (const [kind, type] of cases) {
      expect(widgetTabType(kind)).toBe(type);
    }
  });

  it('无窗口的控件返回 null', () => {
    expect(widgetTabType('Knob')).toBeNull();
    expect(widgetTabType('Gauge')).toBeNull();
    expect(widgetTabType('Custom')).toBeNull();
    expect(widgetTabType('Math')).toBeNull();
    expect(widgetTabType('TableView')).toBeNull();
  });

  it('widgetToTab 构造可关闭窗口 Tab, 名称取 params.label (Waveform 固定名)', () => {
    const raw: WidgetConfig = { kind: 'RawData', params: { id: 'raw-1', label: 'Raw Data' } };
    expect(widgetToTab(raw)).toEqual({
      id: 'raw-1',
      type: 'raw',
      name: 'Raw Data',
      widgetId: 'raw-1',
      closable: true,
    });

    const wave: WidgetConfig = {
      kind: 'Waveform',
      params: { id: 'wave-1', label: 'Waveform', channels: 4, max_points: 10000, visible_channels: [true, true, true, true] },
    };
    expect(widgetToTab(wave)?.name).toBe('Waveform');
  });
});
