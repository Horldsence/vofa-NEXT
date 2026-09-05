// ============ 算术控件属性编辑节 ============
//
// 运算类型 (单目运算隐藏输入数) + 输入端口数 (1..8, 见 MathConfig) + 单位 + 小数位。

import { memo } from 'react';
import type { MathOp } from '../../../types';
import { isUnaryMathOp } from '../../../types';
import { useAppStore } from '../../../store/appStore';
import { t } from '../../../i18n';
import { NumberField, SelectField, TextField } from '../../ui/fields';
import type { WidgetPropertiesProps } from '../registryTypes';

/// 运算选项 — 集合与标签与 WidgetPalette mathItems 同源
const OP_OPTIONS: [MathOp, string][] = [
  ['add', 'mathAdd'],
  ['sub', 'mathSub'],
  ['mul', 'mathMul'],
  ['div', 'mathDiv'],
  ['avg', 'mathAvg'],
  ['min', 'mathMin'],
  ['max', 'mathMax'],
  ['abs', 'mathAbs'],
  ['neg', 'mathNeg'],
  ['square', 'mathSquare'],
  ['sqrt', 'mathSqrt'],
  ['sin', 'mathSin'],
  ['cos', 'mathCos'],
  ['tan', 'mathTan'],
  ['log', 'mathLog'],
];

/// 算术属性 — 运算类型 + 输入数 + 单位 + 小数位
export const MathProperties = memo(function MathProperties({ widget, update }: WidgetPropertiesProps<'Math'>) {
  const lang = useAppStore((s) => s.lang);
  const { op, inputCount, unit, precision } = widget.params;
  const patch = (p: Partial<typeof widget.params>) =>
    update({ kind: 'Math', params: { ...widget.params, ...p } });
  return (
    <>
      <SelectField label={t(lang, 'mathOp')} value={op}
        options={OP_OPTIONS.map(([value, key]) => ({ value, label: t(lang, key) }))}
        onChange={(v) => patch({ op: v as MathOp })} />
      {!isUnaryMathOp(op) && (
        <NumberField label={t(lang, 'mathInputs')} value={inputCount}
          onCommit={(v) => {
            const next = Math.round(v);
            if (next < 1 || next > 8) return false;
            patch({ inputCount: next });
            return true;
          }}
          error={t(lang, 'invalidRange')} />
      )}
      <TextField label={t(lang, 'unit')} value={unit} allowEmpty
        onCommit={(next) => patch({ unit: next })} />
      <NumberField label={t(lang, 'precision')} value={precision}
        onCommit={(v) => {
          const next = Math.round(v);
          if (next < 0 || next > 6) return false;
          patch({ precision: next });
          return true;
        }}
        error={t(lang, 'invalidRange')} />
    </>
  );
});
