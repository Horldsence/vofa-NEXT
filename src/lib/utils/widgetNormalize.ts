// ============ 控件配置归一化 ============
//
// 所有 widget 配置的单一归一化入口: 既迁移旧工作区形态 (如 Gauge 顶层
// min/max → range 量程对象), 也保证从 workspace / graph:source / AI-MCP
// 写入的配置只以当前模型进入 store。幂等 — 已合法的配置原样返回。

import type { ChoiceOption, DisplayRangeConfig, Model3DConfig, WidgetBinding, WidgetConfig } from '../../types';
import { DEFAULT_DISPLAY_RANGE } from '../../types';
import { normalizeCommandConfig } from './commandFrames';
import { snapControlValue } from './numericControl';

type UnknownRecord = Record<string, unknown>;

function asRecord(value: unknown): UnknownRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? value as UnknownRecord
    : {};
}

function finiteOr(value: unknown, fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
}

/// Model3D 配置归一化 — 为旧保存数据补齐姿态格式与模型来源等字段
export function normalizeModel3DConfig(raw: Partial<Model3DConfig>): Model3DConfig {
  const mode =
    raw.mode === 'attitude' ||
    raw.mode === 'trajectory-attitude' ||
    raw.mode === 'trajectory'
      ? raw.mode
      : 'trajectory';
  const attitudeInputMode =
    raw.attitudeInputMode === 'degrees' ||
    raw.attitudeInputMode === 'radians' ||
    raw.attitudeInputMode === 'quaternion'
      ? raw.attitudeInputMode
      : 'radians';
  const color = typeof raw.color === 'string' && /^#[0-9a-fA-F]{6}$/.test(raw.color) ? raw.color : '#75beff';
  const trailLength =
    typeof raw.trailLength === 'number' && raw.trailLength > 0 ? raw.trailLength : 200;
  const axisLength =
    typeof raw.axisLength === 'number' && raw.axisLength > 0 ? raw.axisLength : 1.0;
  const modelSource =
    raw.modelSource?.kind === 'custom' && typeof raw.modelSource.path === 'string'
      ? { kind: 'custom' as const, path: raw.modelSource.path, name: raw.modelSource.name ?? 'model.glb' }
      : { kind: 'builtin-cube' as const };

  return {
    id: raw.id ?? '',
    label: raw.label ?? 'Model3D',
    mode,
    attitudeInputMode,
    trailLength,
    color,
    axisLength,
    modelSource,
  };
}

function normalizeBinding(value: unknown): WidgetBinding {
  const binding = asRecord(value);
  if (binding.mode === 'None') return { mode: 'None' };
  const params = asRecord(binding.params);
  if (
    binding.mode === 'Auto' &&
    typeof params.transportId === 'string' &&
    typeof params.protocolId === 'string'
  ) {
    return {
      mode: 'Auto',
      params: {
        transportId: params.transportId,
        protocolId: params.protocolId,
        channel: Math.max(0, Math.trunc(finiteOr(params.channel, 0))),
      },
    };
  }
  if (
    binding.mode === 'Manual' &&
    typeof params.transportId === 'string'
  ) {
    return {
      mode: 'Manual',
      params: {
        transportId: params.transportId,
        template: typeof params.template === 'string' ? params.template : '{value}',
      },
    };
  }
  // 旧版绑定没有明确目标。宁可禁用，也不能在多接口工作区静默发错设备。
  return { mode: 'None' };
}

function normalizeOptions(value: unknown, widgetId: string): ChoiceOption[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((item, index): ChoiceOption[] => {
    if (Array.isArray(item)) {
      const label = typeof item[0] === 'string' ? item[0] : `Option ${index + 1}`;
      const optionValue = finiteOr(item[1], index);
      return [{ id: `${widgetId}-option-${index + 1}`, label, value: optionValue }];
    }
    const option = asRecord(item);
    if (Object.keys(option).length === 0) return [];
    return [{
      id: typeof option.id === 'string' && option.id !== ''
        ? option.id
        : `${widgetId}-option-${index + 1}`,
      label: typeof option.label === 'string' && option.label.trim() !== ''
        ? option.label
        : `Option ${index + 1}`,
      value: finiteOr(option.value, index),
    }];
  });
}

function normalizeRange(params: UnknownRecord): { min: number; max: number; step: number; value: number } {
  const min = finiteOr(params.min, 0);
  const proposedMax = finiteOr(params.max, 100);
  const max = proposedMax > min ? proposedMax : min + 100;
  const proposedStep = finiteOr(params.step, 1);
  const step = proposedStep > 0 ? proposedStep : 1;
  const rawValue = finiteOr(params.value, finiteOr(params.default, min));
  return { min, max, step, value: snapControlValue(rawValue, { min, max, step }) };
}

// ============ 显示量程/刻度 ============

/// 显示量程归一化 — 幂等; 旧配置 (range 缺失, 顶层 min/max) 迁移为 manual 量程
export function normalizeDisplayRange(rawRange: unknown, legacy: UnknownRecord): DisplayRangeConfig {
  const raw = asRecord(rawRange);
  // 旧 Gauge/Progress: 顶层 min/max → manual 量程
  const legacyMin = typeof legacy.min === 'number' && Number.isFinite(legacy.min) ? legacy.min : null;
  const legacyMax = typeof legacy.max === 'number' && Number.isFinite(legacy.max) ? legacy.max : null;
  const min = legacyMin ?? finiteOr(raw.min, DEFAULT_DISPLAY_RANGE.min);
  const proposedMax = legacyMax ?? finiteOr(raw.max, DEFAULT_DISPLAY_RANGE.max);
  const max = proposedMax > min ? proposedMax : min + 100;
  const rawWindow = finiteOr(raw.windowSec, DEFAULT_DISPLAY_RANGE.windowSec);
  const precision = raw.precision === 'auto' || typeof raw.precision === 'number'
    ? raw.precision
    : DEFAULT_DISPLAY_RANGE.precision;
  return {
    mode: raw.mode === 'auto' ? 'auto' : 'manual',
    min,
    max,
    windowSec: Math.min(3600, Math.max(1, rawWindow > 0 ? rawWindow : 10)),
    majorTicks: Math.min(11, Math.max(2, Math.round(finiteOr(raw.majorTicks, DEFAULT_DISPLAY_RANGE.majorTicks)))),
    precision: precision === 'auto'
      ? 'auto'
      : Math.max(0, Math.min(6, Math.round(precision))),
  };
}

/**
 * 所有 widget 配置的单一归一化入口。它既迁移旧输入控件形态，也保证从
 * workspace / graph:source / AI-MCP 写入的配置只以当前模型进入 store。
 */
export function normalizeWidgetConfig(widget: WidgetConfig): WidgetConfig {
  const params = asRecord(widget.params);
  const id = typeof params.id === 'string' ? params.id : '';
  const fallbackLabel = widget.kind === 'Label' && typeof params.text === 'string'
    ? params.text
    : widget.kind;
  const label = typeof params.label === 'string' && params.label.trim() !== ''
    ? params.label
    : fallbackLabel;

  switch (widget.kind) {
    case 'Knob':
    case 'Slider': {
      const range = normalizeRange(params);
      return {
        kind: widget.kind,
        params: { id, label, ...range, binding: normalizeBinding(params.binding) },
      };
    }
    case 'Button':
      return {
        kind: 'Button',
        params: {
          id,
          label,
          pressValue: finiteOr(params.pressValue, finiteOr(params.press_value, 1)),
          releaseValue: finiteOr(params.releaseValue, finiteOr(params.release_value, 0)),
          binding: normalizeBinding(params.binding),
        },
      };
    case 'Radio': {
      const options = normalizeOptions(params.options, id);
      const safeOptions = options.length > 0
        ? options
        : [{ id: `${id}-option-1`, label: 'Option 1', value: 0 }];
      const legacyIndex = Math.max(0, Math.trunc(finiteOr(params.default, 0)));
      const requestedId = typeof params.selectedId === 'string' ? params.selectedId : '';
      const selectedId = safeOptions.some((option) => option.id === requestedId)
        ? requestedId
        : (safeOptions[legacyIndex]?.id ?? safeOptions[0].id);
      return {
        kind: 'Radio',
        params: { id, label, options: safeOptions, selectedId, binding: normalizeBinding(params.binding) },
      };
    }
    case 'Checkbox': {
      const isLegacy = 'checked_value' in params || 'unchecked_value' in params || 'default' in params;
      const options = isLegacy
        ? [{
            id: `${id}-option-1`,
            label: 'Option 1',
            value: finiteOr(params.checked_value, 1),
          }]
        : normalizeOptions(params.options, id);
      const safeOptions = options.length > 0
        ? options
        : [{ id: `${id}-option-1`, label: 'Option 1', value: 1 }];
      const validIds = new Set(safeOptions.map((option) => option.id));
      const selectedIds = isLegacy
        ? (params.default === true ? [safeOptions[0].id] : [])
        : (Array.isArray(params.selectedIds)
            ? params.selectedIds.filter((item): item is string => typeof item === 'string' && validIds.has(item))
            : []);
      const emptyValue = isLegacy
        ? finiteOr(params.unchecked_value, 0)
        : finiteOr(params.emptyValue, 0);
      return {
        kind: 'Checkbox',
        params: {
          id,
          label,
          options: safeOptions,
          selectedIds,
          ...(emptyValue === 0 ? {} : { emptyValue }),
          binding: normalizeBinding(params.binding),
        },
      };
    }
    case 'Label':
      return {
        kind: 'Label',
        params: {
          id,
          label,
          text: typeof params.text === 'string' ? params.text : 'Label',
          channel: typeof params.channel === 'number' ? params.channel : null,
        },
      };
    case 'Gauge':
      return {
        kind: 'Gauge',
        params: {
          id,
          label,
          range: normalizeDisplayRange(params.range, params),
          unit: typeof params.unit === 'string' ? params.unit : '',
          channel: typeof params.channel === 'number' ? params.channel : null,
        },
      };
    case 'Progress': {
      const orientation = params.orientation === 'vertical' ? 'vertical' : 'horizontal';
      const color = typeof params.color === 'string' && /^#[0-9a-fA-F]{6}$/.test(params.color)
        ? params.color
        : '';
      return {
        kind: 'Progress',
        params: {
          id,
          label,
          range: normalizeDisplayRange(params.range, params),
          unit: typeof params.unit === 'string' ? params.unit : '',
          orientation,
          showValue: params.showValue !== false,
          color,
          channel: typeof params.channel === 'number' ? params.channel : null,
        },
      };
    }
    case 'Waveform':
      return {
        kind: 'Waveform',
        params: { ...widget.params, id, label },
      };
    case 'Command':
      return { kind: 'Command', params: normalizeCommandConfig({ ...params, id, label } as never) };
    case 'Model3D':
      return { kind: 'Model3D', params: normalizeModel3DConfig({ ...params, id, label }) };
    default:
      return {
        ...widget,
        params: { ...widget.params, id, label },
      } as WidgetConfig;
  }
}
