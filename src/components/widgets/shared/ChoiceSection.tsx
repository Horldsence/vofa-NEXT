// ============ 选项编辑节 (Radio / Checkbox 共用) ============
//
// 自 nodes/WidgetProperties.tsx 的 ChoiceEditor 泛型化搬出, 与 BindingSection 同构:
// 泛型保持调用方的 kind 收窄, 避免上层 update 丢类型。
// 选项增删/排序/改值/勾选均经 widgetInputValue 计算控件值, 按需经 sendBindingValue 下发。

import { ArrowDown, ArrowUp, Plus, Trash2 } from 'lucide-react';
import { nanoid } from 'nanoid';
import type { ChoiceOption, WidgetConfig } from '../../../types';
import { t } from '../../../i18n';
import { useAppStore } from '../../../store/appStore';
import { widgetInputValue } from '../../../lib/utils/widgetDefaults';
import { sendBindingValue } from './binding';
import { NumberField, TextField } from '../../ui/fields';

export type ChoiceKind = 'Radio' | 'Checkbox';
type ChoiceWidget<K extends ChoiceKind> = Extract<WidgetConfig, { kind: K }>;
/// 泛型 K 下 Extract 是延迟条件类型 (取不到属性), 内部统一用具体联合类型
type AnyChoiceWidget = Extract<WidgetConfig, { kind: ChoiceKind }>;

interface ChoiceSectionProps<K extends ChoiceKind> {
  widget: ChoiceWidget<K>;
  update: (widget: ChoiceWidget<K>) => void;
}

export function ChoiceSection<K extends ChoiceKind>({ widget: narrowedWidget, update }: ChoiceSectionProps<K>) {
  const widget = narrowedWidget as AnyChoiceWidget;
  const updateNarrowed = (next: AnyChoiceWidget) => update(next as ChoiceWidget<K>);
  const lang = useAppStore((s) => s.lang);
  const changeOptions = (options: ChoiceOption[], notifyValueChange = false) => {
    let next: AnyChoiceWidget;
    if (widget.kind === 'Radio') {
      const selectedId = options.some((option) => option.id === widget.params.selectedId) ? widget.params.selectedId : options[0].id;
      next = { kind: 'Radio', params: { ...widget.params, options, selectedId } };
    } else {
      const ids = new Set(options.map((option) => option.id));
      next = { kind: 'Checkbox', params: { ...widget.params, options, selectedIds: widget.params.selectedIds.filter((id) => ids.has(id)) } };
    }
    const oldValue = widgetInputValue(widget);
    updateNarrowed(next);
    const nextValue = widgetInputValue(next);
    if (notifyValueChange && nextValue !== null && nextValue !== oldValue) sendBindingValue(next.params.binding, nextValue);
  };
  const select = (next: AnyChoiceWidget) => {
    updateNarrowed(next);
    const value = widgetInputValue(next);
    if (value !== null) sendBindingValue(next.params.binding, value);
  };
  return (
    <section className="mt-3 pt-3 border-t border-border">
      <div className="flex items-center justify-between mb-2">
        <span className="text-[10px] font-semibold uppercase tracking-wider text-text-secondary">{t(lang, 'options')}</span>
        <button type="button" className="w-6 h-6 flex items-center justify-center rounded hover:bg-bg-hover"
          onClick={() => changeOptions([...widget.params.options, { id: nanoid(8), label: `Option ${widget.params.options.length + 1}`, value: widget.params.options.length }])}
          title={t(lang, 'addOption')}><Plus size={13} /></button>
      </div>
      <div className="flex flex-col gap-2">{widget.params.options.map((option, index) => (
        <div key={option.id} className="p-2 rounded border border-border bg-bg-input/40">
          <div className="flex gap-1 mb-1.5">
            <input type={widget.kind === 'Radio' ? 'radio' : 'checkbox'}
              checked={widget.kind === 'Radio' ? widget.params.selectedId === option.id : widget.params.selectedIds.includes(option.id)}
              onChange={() => {
                if (widget.kind === 'Radio') select({ kind: 'Radio', params: { ...widget.params, selectedId: option.id } });
                else {
                  const selected = new Set(widget.params.selectedIds);
                  if (selected.has(option.id)) selected.delete(option.id); else selected.add(option.id);
                  select({ kind: 'Checkbox', params: { ...widget.params, selectedIds: [...selected] } });
                }
              }} />
            <button type="button" disabled={index === 0} onClick={() => {
              const options = [...widget.params.options]; [options[index - 1], options[index]] = [options[index], options[index - 1]]; changeOptions(options);
            }}><ArrowUp size={12} /></button>
            <button type="button" disabled={index === widget.params.options.length - 1} onClick={() => {
              const options = [...widget.params.options]; [options[index], options[index + 1]] = [options[index + 1], options[index]]; changeOptions(options);
            }}><ArrowDown size={12} /></button>
            <button type="button" disabled={widget.params.options.length <= 1}
              onClick={() => {
                const options = widget.params.options.filter((item) => item.id !== option.id);
                if (widget.kind === 'Radio') {
                  const selectedId = widget.params.selectedId === option.id
                    ? options[Math.min(index, options.length - 1)].id
                    : widget.params.selectedId;
                  const next: AnyChoiceWidget = { kind: 'Radio', params: { ...widget.params, options, selectedId } };
                  if (selectedId !== widget.params.selectedId) select(next); else updateNarrowed(next);
                } else {
                  const wasSelected = widget.params.selectedIds.includes(option.id);
                  const next: AnyChoiceWidget = {
                    kind: 'Checkbox',
                    params: { ...widget.params, options, selectedIds: widget.params.selectedIds.filter((id) => id !== option.id) },
                  };
                  if (wasSelected) select(next); else updateNarrowed(next);
                }
              }}><Trash2 size={12} /></button>
          </div>
          <TextField label={t(lang, 'optionName')} value={option.label} onCommit={(label) => {
            changeOptions(widget.params.options.map((item) => item.id === option.id ? { ...item, label } : item));
          }} />
          <NumberField label={t(lang, 'optionValue')} value={option.value} onCommit={(value) => {
            changeOptions(widget.params.options.map((item) => item.id === option.id ? { ...item, value } : item), true); return true;
          }} />
        </div>
      ))}</div>
    </section>
  );
}
