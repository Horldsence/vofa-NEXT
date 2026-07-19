import { nanoid } from 'nanoid';
import type { WidgetConfig, WindowType, SpectrumOutput } from '../types';

/// Custom widget 编辑器默认代码 (与 CustomWidgetEditor 中常量保持一致)
export const DEFAULT_CUSTOM_CODE = `({
  name: 'MyWidget',
  description: '自定义控件示例',
  inputs: [
    { id: 'value', label: 'Value' }
  ],
  outputs: [],
  settings: [
    { id: 'unit', label: 'Unit', type: 'text', default: 'V' },
    { id: 'color', label: 'Color', type: 'color', default: '#75beff' }
  ],
  onMount: function(ctx) {
    ctx.state.count = 0;
  },
  render: function(ctx) {
    const v = ctx.inputs.value ?? 0;
    const u = ctx.settings.unit || '';
    const c = ctx.settings.color || '#75beff';
    ctx.el.innerHTML =
      '<div style="padding:8px;text-align:center;font-family:sans-serif">' +
        '<div style="font-size:24px;color:' + c + ';font-weight:bold">' +
          Number(v).toFixed(2) +
        '</div>' +
        '<div style="font-size:11px;color:#888">' + u + '</div>' +
      '</div>';
  }
})
`;

/// 辅助函数: 创建控件
export function createWidget(kind: WidgetConfig['kind']): WidgetConfig {
  const id = nanoid(8);
  switch (kind) {
    case 'Knob':
      return {
        kind: 'Knob',
        params: {
          id, label: 'Knob', min: 0, max: 100, step: 1, default: 50,
          binding: { mode: 'None' },
        },
      };
    case 'Button':
      return {
        kind: 'Button',
        params: {
          id, label: 'Button', press_value: 1, release_value: 0,
          binding: { mode: 'None' },
        },
      };
    case 'Radio':
      return {
        kind: 'Radio',
        params: {
          id, label: 'Radio', options: [['A', 0], ['B', 1]], default: 0,
          binding: { mode: 'None' },
        },
      };
    case 'Checkbox':
      return {
        kind: 'Checkbox',
        params: {
          id, label: 'Checkbox', checked_value: 1, unchecked_value: 0, default: false,
          binding: { mode: 'None' },
        },
      };
    case 'Slider':
      return {
        kind: 'Slider',
        params: {
          id, label: 'Slider', min: 0, max: 100, step: 1, default: 50,
          binding: { mode: 'None' },
        },
      };
    case 'Label':
      return {
        kind: 'Label',
        params: { id, text: 'Label', channel: null },
      };
    case 'Waveform':
      return {
        kind: 'Waveform',
        params: { id, channels: 4, max_points: 10000, visible_channels: [true, true, true, true] },
      };
    case 'PieChart':
      return {
        kind: 'PieChart',
        params: { id, label: 'Pie', segments: ['A', 'B', 'C'], channels: [0, 1, 2] },
      };
    case 'Image':
      return {
        kind: 'Image',
        params: { id, label: 'Image', width: 320, height: 240, format: 'rgb888' },
      };
    case 'Gauge':
      return {
        kind: 'Gauge',
        params: { id, label: 'Gauge', min: 0, max: 100, unit: '', channel: null },
      };
    case 'LED':
      return {
        kind: 'LED',
        params: {
          id, label: 'LED', threshold: 0.5,
          on_color: '#89d185', off_color: '#3c3c3c', channel: null,
        },
      };
    case 'NumberDisplay':
      return {
        kind: 'NumberDisplay',
        params: { id, label: 'Value', unit: '', precision: 2, channel: null },
      };
    case 'Custom':
      return {
        kind: 'Custom',
        params: { id, label: 'Custom', code: DEFAULT_CUSTOM_CODE, settings: {} },
      };
    case 'Math':
      return {
        kind: 'Math',
        params: {
          id,
          label: 'Math',
          op: 'add',
          inputCount: 2,
          unit: '',
          precision: 3,
        },
      };
    case 'Filter':
      return {
        kind: 'Filter',
        params: {
          id,
          label: 'Filter',
          preset: 'Lowpass',
          cutoff: 100,
          low: 80,
          high: 200,
          sampleRate: 1000,
          precision: 3,
        },
      };
    case 'Spectrum':
      return {
        kind: 'Spectrum',
        params: {
          id,
          label: 'Spectrum',
          windowSize: 512,
          windowType: 'Hann' as WindowType,
          output: 'Magnitude' as SpectrumOutput,
          sampleRate: 1000,
        },
      };
    case 'Model3D':
      return {
        kind: 'Model3D',
        params: {
          id,
          label: 'Model3D',
          mode: 'trajectory',
          trailLength: 200,
          color: '#75beff',
          axisLength: 1.0,
        },
      };
    case 'Command':
      return {
        kind: 'Command',
        params: {
          id,
          label: 'Command',
          blocks: [
            { id: 'b1', type: 'const_hex', label: '帧头', hex: 'AA 01' },
            { id: 'b2', type: 'var_ref', label: '速度', portName: 'speed', fieldType: 'uint16LE' },
            { id: 'b3', type: 'checksum', label: '校验', checksum: 'sum8' },
          ],
          appendNewline: false,
          loopbackEnabled: false,
          loopbackSendMode: 'manual',
          loopbackTimerMs: 100,
          loopbackHistory: [],
        },
      };
    case 'TableView':
      return {
        kind: 'TableView',
        params: {
          id,
          label: 'Table',
          columns: [
            { portName: 'ch0', label: 'CH0', showRaw: true },
            { portName: 'ch1', label: 'CH1', showRaw: true },
          ],
          maxRows: 100,
          showRawData: true,
          showTimestamp: true,
        },
      };
    case 'FrameDecoder':
      return {
        kind: 'FrameDecoder',
        params: {
          id,
          label: 'FrameDecoder',
          blocks: [
            { id: 'b1', type: 'header', label: '帧头', hex: 'AA' },
            { id: 'b2', type: 'field', label: '字段1', fieldType: 'uint8', portName: 'field_1' },
            { id: 'b3', type: 'field', label: '字段2', fieldType: 'uint8', portName: 'field_2' },
            { id: 'b4', type: 'checksum', label: '校验', algorithm: 'sum8', cover: 'all_prior', position: 'append' },
          ],
          enableValid: true,
          enableFrameCount: false,
          enableLastTimestamp: false,
          enableFps: false,
          mode: 'live',
        },
      };
  }
}
