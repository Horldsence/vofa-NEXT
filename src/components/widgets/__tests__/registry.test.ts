// ============ Widget 注册表完整性守卫 ============
//
// registry 的 Record<WidgetKind, WidgetDef> 在编译期强制穷举 union;
// 本测试在运行期兜底各平行站点 (默认工厂 / 端口表 / 尺寸表 / 归一化) 的
// 一致性 — 新增控件 kind 时任何一处遗漏都会在这里 (或编译期) 暴露。

import { describe, expect, it } from 'vitest';
import { WIDGET_DEFS, WIDGET_REGISTRY, type WidgetKind } from '../registry';
import { createWidget, widgetInputValue } from '../../../lib/utils/widgetDefaults';
import { normalizeWidgetConfig } from '../../../lib/utils/widgetNormalize';
import { getWidgetPorts } from '../../nodes/WidgetPorts';
import { WIDGET_SIZE_LIMITS } from '../../../lib/utils/widgetSize';
import { t } from '../../../i18n';
import type { WidgetConfig } from '../../../types';

const ALL_KINDS = Object.keys(WIDGET_REGISTRY) as WidgetKind[];

describe('widget registry completeness', () => {
  it('every def has component, icon, labelKey and valid i18n label', () => {
    for (const def of WIDGET_DEFS) {
      // memo 组件是 ExoticComponent 对象 — 只能断言「已挂载实现」
      expect(def.Component, `${def.kind}.Component`).toBeDefined();
      expect(def.icon, `${def.kind}.icon`).toBeDefined();
      expect(def.labelKey, `${def.kind}.labelKey`).toBeTypeOf('string');
      // i18n t 缺键回退 key 本身 — 回退即视为漏配
      expect(t('en', def.labelKey as never), `${def.kind}.labelKey(en)`).not.toBe(def.labelKey);
      expect(t('zh', def.labelKey as never), `${def.kind}.labelKey(zh)`).not.toBe(def.labelKey);
    }
  });

  it('every kind has a default factory, ports and size limits', () => {
    for (const kind of ALL_KINDS) {
      const widget = createWidget(kind);
      expect(widget.kind).toBe(kind);
      expect(widget.params.id, `${kind}.id`).toBeTypeOf('string');
      const ports = getWidgetPorts(widget);
      expect(Array.isArray(ports.inputs)).toBe(true);
      expect(Array.isArray(ports.outputs)).toBe(true);
      expect(WIDGET_SIZE_LIMITS[kind], `${kind}.sizeLimits`).toBeDefined();
    }
  });

  it('normalizeWidgetConfig is idempotent for every default factory output', () => {
    for (const kind of ALL_KINDS) {
      const widget = createWidget(kind);
      const once = normalizeWidgetConfig(widget);
      const twice = normalizeWidgetConfig(once);
      expect(twice, kind).toEqual(once);
    }
  });

  it('display range config survives normalize on Gauge / Progress', () => {
    for (const kind of ['Gauge', 'Progress'] as const) {
      const widget = normalizeWidgetConfig(createWidget(kind));
      const params = widget.params as unknown as { range: { mode: string; min: number; max: number; majorTicks: number } };
      expect(params.range.mode).toBe('manual');
      expect(params.range.min).toBeLessThan(params.range.max);
      expect(params.range.majorTicks).toBeGreaterThanOrEqual(2);
    }
  });

  it('registry kinds match the AI tool whitelist', async () => {
    // lib/ai/toolHost 的 WIDGET_KINDS 是运行时校验用的独立清单 — 必须与 registry 同步
    const { WIDGET_KINDS } = await import('../../../lib/ai/toolHost');
    expect([...ALL_KINDS].sort()).toEqual([...WIDGET_KINDS].sort());
  });

  it('input value helpers stay null for non-input kinds', () => {
    const nonInput: WidgetConfig['kind'][] = ['Gauge', 'Progress', 'Waveform', 'Label'];
    for (const kind of nonInput) {
      expect(widgetInputValue(createWidget(kind))).toBeNull();
    }
  });
});
