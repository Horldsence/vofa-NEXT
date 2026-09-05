import { memo } from 'react';
import { useAppStore } from '../../../store/appStore';
import type { WidgetBinding } from '../../../types';
import { t } from '../../../i18n';
import { snapControlValue, validateNumericRange } from '../../../lib/utils/numericControl';
import { sendBindingValue } from './binding';
import { NumberField } from '../../ui/fields';

/// Knob / Slider 共用的量程参数结构 (两者 params 结构等价)
export interface ControlRangeParams {
  id: string;
  min: number;
  max: number;
  step: number;
  value: number;
  binding: WidgetBinding;
}

interface ControlRangeEditorProps {
  params: ControlRangeParams;
  /// 写回完整 patch (调用方包一层 kind 包装)
  onPatch: (patch: Partial<ControlRangeParams>) => void;
}

/// 数值输入控件量程节 — min/max/step/currentValue; 量程变化时把当前值
/// 吸附进新量程并经绑定通道补发 (与拖拽改值同一语义)。
export const ControlRangeEditor = memo(function ControlRangeEditor({ params, onPatch }: ControlRangeEditorProps) {
  const lang = useAppStore((s) => s.lang);
  const commitInputValue = useAppStore((s) => s.commitInputValue);
  const patchRange = (changes: Partial<Pick<ControlRangeParams, 'min' | 'max' | 'step'>>) => {
    const range = {
      min: changes.min ?? params.min,
      max: changes.max ?? params.max,
      step: changes.step ?? params.step,
    };
    if (validateNumericRange(range)) return false;
    const value = snapControlValue(params.value, range);
    onPatch({ ...range, value });
    if (value !== params.value) sendBindingValue(params.binding, value);
    return true;
  };
  return (
    <>
      <NumberField label={t(lang, 'minValue')} value={params.min} onCommit={(min) => patchRange({ min })} error={t(lang, 'invalidRange')} />
      <NumberField label={t(lang, 'maxValue')} value={params.max} onCommit={(max) => patchRange({ max })} error={t(lang, 'invalidRange')} />
      <NumberField label={t(lang, 'step')} value={params.step} onCommit={(step) => patchRange({ step })} error={t(lang, 'invalidStep')} />
      <NumberField label={t(lang, 'currentValue')} value={params.value} onCommit={(value) => {
        const normalized = snapControlValue(value, params);
        commitInputValue(params.id, normalized);
        sendBindingValue(params.binding, normalized);
        return true;
      }} error={t(lang, 'invalidValue')} />
    </>
  );
});
