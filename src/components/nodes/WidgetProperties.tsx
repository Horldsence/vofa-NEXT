import { memo, useEffect, useState } from 'react';
import type { Node } from '@xyflow/react';
import { ArrowDown, ArrowUp, Code2, Plus, Trash2 } from 'lucide-react';
import { nanoid } from 'nanoid';
import { useAppStore } from '../../store/appStore';
import type { ChoiceOption, WidgetBinding, WidgetConfig } from '../../types';
import { t } from '../../i18n';
import { snapControlValue, validateNumericRange } from '../../lib/utils/numericControl';
import { widgetInputValue } from '../../lib/utils/createWidget';
import { sendBindingValue } from '../controls/binding';

function TextField({ value, label, onCommit }: { value: string; label: string; onCommit: (value: string) => void }) {
  const lang = useAppStore((s) => s.lang);
  const [draft, setDraft] = useState(value);
  const [invalid, setInvalid] = useState(false);
  useEffect(() => { setDraft(value); setInvalid(false); }, [value]);
  const commit = () => {
    const next = draft.trim();
    if (next === '') {
      setInvalid(true);
      return;
    }
    setInvalid(false);
    setDraft(next);
    if (next !== value) onCommit(next);
  };
  return (
    <label className="block mb-2">
      <span className="block text-xs text-text-secondary mb-1">{label}</span>
      <input className={`form-input ${invalid ? 'border-red' : ''}`} value={draft}
        onChange={(event) => { setDraft(event.target.value); setInvalid(false); }} onBlur={commit}
        onKeyDown={(event) => { if (event.key === 'Enter') { event.preventDefault(); commit(); } }}
        aria-invalid={invalid} />
      {invalid && <span className="block mt-1 text-[10px] text-red">{t(lang, 'requiredValue')}</span>}
    </label>
  );
}

function NumberField({ value, label, onCommit, error }: {
  value: number; label: string; onCommit: (value: number) => boolean; error?: string;
}) {
  const [draft, setDraft] = useState(String(value));
  const [invalid, setInvalid] = useState(false);
  useEffect(() => { setDraft(String(value)); setInvalid(false); }, [value]);
  const commit = () => {
    if (draft.trim() === '') {
      setInvalid(true);
      return;
    }
    const parsed = Number(draft);
    const ok = Number.isFinite(parsed) && (parsed === value || onCommit(parsed));
    setInvalid(!ok);
    if (ok) setDraft(String(parsed));
  };
  return (
    <label className="block mb-2">
      <span className="block text-xs text-text-secondary mb-1">{label}</span>
      <input type="number" className={`form-input font-mono ${invalid ? 'border-red' : ''}`} value={draft}
        onChange={(event) => { setDraft(event.target.value); setInvalid(false); }} onBlur={commit}
        onKeyDown={(event) => { if (event.key === 'Enter') { event.preventDefault(); commit(); } }}
        aria-invalid={invalid} />
      {invalid && <span className="block mt-1 text-[10px] text-red">{error ?? 'Invalid value'}</span>}
    </label>
  );
}

type InputWidget = Extract<WidgetConfig, { kind: 'Knob' | 'Slider' | 'Button' | 'Radio' | 'Checkbox' }>;

function nodeDisplayName(node: Node): string {
  return typeof node.data.label === 'string' ? node.data.label : node.id;
}

function BindingEditor({ widget, update }: { widget: InputWidget; update: (widget: InputWidget) => void }) {
  const lang = useAppStore((s) => s.lang);
  const nodes = useAppStore((s) => s.rfNodes);
  const edges = useAppStore((s) => s.rfEdges);
  const binding = widget.params.binding;
  const transports = nodes.filter((node) => node.type === 'transport');
  const transportId = binding.mode === 'Auto' || binding.mode === 'Manual' ? binding.params.transportId : '';
  const downstreamIds = new Set(edges.filter((edge) => edge.source === transportId).map((edge) => edge.target));
  const protocols = nodes.filter((node) => node.type === 'protocol' && downstreamIds.has(node.id));
  const invalidTarget = binding.mode !== 'None' && !transports.some((node) => node.id === transportId);
  const selectedProtocol = binding.mode === 'Auto'
    ? protocols.find((node) => node.id === binding.params.protocolId)
    : undefined;
  const invalidProtocol = binding.mode === 'Auto' && (
    !selectedProtocol || (selectedProtocol.data.config as { kind?: string } | undefined)?.kind === 'RawData'
  );
  const setBinding = (next: WidgetBinding) => update({ ...widget, params: { ...widget.params, binding: next } } as InputWidget);

  return (
    <section className="mt-3 pt-3 border-t border-border">
      <div className="text-[10px] font-semibold uppercase tracking-wider text-text-secondary mb-2">{t(lang, 'bindingMode')}</div>
      <select className="form-select mb-2" value={binding.mode} onChange={(event) => {
        const mode = event.target.value;
        if (mode === 'None') setBinding({ mode: 'None' });
        else if (mode === 'Manual') setBinding({ mode: 'Manual', params: { transportId: '', template: '{value}' } });
        else setBinding({ mode: 'Auto', params: { transportId: '', protocolId: '', channel: 0 } });
      }}>
        <option value="None">{t(lang, 'none')}</option><option value="Auto">{t(lang, 'auto')}</option><option value="Manual">{t(lang, 'manual')}</option>
      </select>
      {binding.mode !== 'None' && <>
        <label className="block text-xs text-text-secondary mb-1">{t(lang, 'bindingTransport')}</label>
        <select className="form-select mb-2" value={binding.params.transportId} onChange={(event) => {
          const nextTransport = event.target.value;
          if (binding.mode === 'Manual') setBinding({ ...binding, params: { ...binding.params, transportId: nextTransport } });
          else setBinding({ ...binding, params: { ...binding.params, transportId: nextTransport, protocolId: '' } });
        }}>
          <option value="">—</option>
          {transports.map((node) => <option key={node.id} value={node.id}>{nodeDisplayName(node)}</option>)}
        </select>
      </>}
      {binding.mode === 'Auto' && <>
        <label className="block text-xs text-text-secondary mb-1">{t(lang, 'bindingProtocol')}</label>
        <select className="form-select mb-2" value={binding.params.protocolId}
          onChange={(event) => setBinding({ ...binding, params: { ...binding.params, protocolId: event.target.value } })}>
          <option value="">—</option>{protocols.map((node) => <option key={node.id} value={node.id}>{nodeDisplayName(node)}</option>)}
        </select>
        <NumberField label={t(lang, 'channel')} value={binding.params.channel} onCommit={(channel) => {
          if (!Number.isInteger(channel) || channel < 0) return false;
          setBinding({ ...binding, params: { ...binding.params, channel } }); return true;
        }} />
      </>}
      {binding.mode === 'Manual' && <>
        <TextField label={t(lang, 'template')} value={binding.params.template}
          onCommit={(template) => setBinding({ ...binding, params: { ...binding.params, template } })} />
        <div className="text-[10px] text-text-secondary">{t(lang, 'bindingTemplateHint')}</div>
      </>}
      {(invalidTarget || invalidProtocol) &&
        <div className="mt-1 text-[10px] text-red">{t(lang, 'bindingInvalidTarget')}</div>}
    </section>
  );
}

type ChoiceWidget = Extract<WidgetConfig, { kind: 'Radio' | 'Checkbox' }>;

function ChoiceEditor({ widget, update }: { widget: ChoiceWidget; update: (widget: ChoiceWidget) => void }) {
  const lang = useAppStore((s) => s.lang);
  const changeOptions = (options: ChoiceOption[], notifyValueChange = false) => {
    let next: ChoiceWidget;
    if (widget.kind === 'Radio') {
      const selectedId = options.some((option) => option.id === widget.params.selectedId) ? widget.params.selectedId : options[0].id;
      next = { kind: 'Radio', params: { ...widget.params, options, selectedId } };
    } else {
      const ids = new Set(options.map((option) => option.id));
      next = { kind: 'Checkbox', params: { ...widget.params, options, selectedIds: widget.params.selectedIds.filter((id) => ids.has(id)) } };
    }
    const oldValue = widgetInputValue(widget);
    update(next);
    const nextValue = widgetInputValue(next);
    if (notifyValueChange && nextValue !== null && nextValue !== oldValue) sendBindingValue(next.params.binding, nextValue);
  };
  const select = (next: ChoiceWidget) => {
    update(next);
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
                  const next: ChoiceWidget = { kind: 'Radio', params: { ...widget.params, options, selectedId } };
                  if (selectedId !== widget.params.selectedId) select(next); else update(next);
                } else {
                  const wasSelected = widget.params.selectedIds.includes(option.id);
                  const next: ChoiceWidget = {
                    kind: 'Checkbox',
                    params: { ...widget.params, options, selectedIds: widget.params.selectedIds.filter((id) => id !== option.id) },
                  };
                  if (wasSelected) select(next); else update(next);
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

export const WidgetProperties = memo(function WidgetProperties({ node }: { node: Node }) {
  const lang = useAppStore((s) => s.lang);
  const widget = useAppStore((s) => s.widgets.find((item) => item.params.id === node.id));
  const updateWidget = useAppStore((s) => s.updateWidget);
  const commitInputValue = useAppStore((s) => s.commitInputValue);
  const openCustomEditor = useAppStore((s) => s.openCustomEditor);
  if (!widget) return null;
  const update = (next: WidgetConfig) => updateWidget(widget.params.id, next);
  return (
    <div className="absolute top-2 right-2 bottom-2 w-[300px] z-20 bg-bg-sidebar border border-border rounded-md shadow-lg overflow-y-auto p-3">
      <div className="text-[10px] font-semibold uppercase tracking-wider text-text-secondary mb-2">{widget.kind}</div>
      <TextField label={t(lang, 'widgetName')} value={widget.params.label}
        onCommit={(label) => update({ ...widget, params: { ...widget.params, label } } as WidgetConfig)} />
      {(widget.kind === 'Knob' || widget.kind === 'Slider') && (() => {
        const params = widget.params;
        const patchRange = (changes: Partial<Pick<typeof params, 'min' | 'max' | 'step'>>) => {
          const range = { min: changes.min ?? params.min, max: changes.max ?? params.max, step: changes.step ?? params.step };
          if (validateNumericRange(range)) return false;
          const value = snapControlValue(params.value, range);
          update({ kind: widget.kind, params: { ...params, ...range, value } });
          if (value !== params.value) sendBindingValue(params.binding, value);
          return true;
        };
        return <>
          <NumberField label={t(lang, 'minValue')} value={params.min} onCommit={(min) => patchRange({ min })} error={t(lang, 'invalidRange')} />
          <NumberField label={t(lang, 'maxValue')} value={params.max} onCommit={(max) => patchRange({ max })} error={t(lang, 'invalidRange')} />
          <NumberField label={t(lang, 'step')} value={params.step} onCommit={(step) => patchRange({ step })} error={t(lang, 'invalidStep')} />
          <NumberField label={t(lang, 'currentValue')} value={params.value} onCommit={(value) => {
            const normalized = snapControlValue(value, params);
            commitInputValue(params.id, normalized);
            sendBindingValue(params.binding, normalized);
            return true;
          }} />
          <BindingEditor widget={widget} update={(next) => update(next)} />
        </>;
      })()}
      {widget.kind === 'Button' && <>
        <NumberField label={t(lang, 'press')} value={widget.params.pressValue} onCommit={(pressValue) => { update({ kind: 'Button', params: { ...widget.params, pressValue } }); return true; }} />
        <NumberField label={t(lang, 'release')} value={widget.params.releaseValue} onCommit={(releaseValue) => { update({ kind: 'Button', params: { ...widget.params, releaseValue } }); return true; }} />
        <BindingEditor widget={widget} update={(next) => update(next)} />
      </>}
      {(widget.kind === 'Radio' || widget.kind === 'Checkbox') && <>
        <ChoiceEditor widget={widget} update={(next) => update(next)} />
        <BindingEditor widget={widget} update={(next) => update(next)} />
      </>}
      {widget.kind === 'Label' && <TextField label={t(lang, 'labelText')} value={widget.params.text}
        onCommit={(text) => update({ kind: 'Label', params: { ...widget.params, text } })} />}
      {widget.kind === 'Custom' && <button type="button" className="w-full h-8 mt-2 bg-bg-button text-text-inverse rounded inline-flex items-center justify-center gap-1.5"
        onClick={() => openCustomEditor(widget.params.id)}><Code2 size={14} /> {t(lang, 'customWidgetEditor')}</button>}
    </div>
  );
});
