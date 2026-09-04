import { memo } from 'react';
import type { Node } from '@xyflow/react';
import { ArrowDown, ArrowUp, Code2, Plus, RotateCcw, Trash2 } from 'lucide-react';
import { nanoid } from 'nanoid';
import { useAppStore } from '../../store/appStore';
import type {
  ChoiceOption,
  FilterPresetKind,
  SpectrumOutput,
  WidgetBinding,
  WidgetConfig,
  WindowType,
} from '../../types';
import { STR_OP_PORTS } from '../../types';
import { t } from '../../i18n';
import { snapControlValue, validateNumericRange } from '../../lib/utils/numericControl';
import { widgetInputValue } from '../../lib/utils/createWidget';
import { clampWidgetSize, widgetMinSize } from '../../lib/utils/widgetSize';
import { sendBindingValue } from '../controls/binding';
import { NumberField, OptionalNumberField, SelectField, TextField } from '../ui/fields';

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

/// 节点尺寸编辑 — 宽/高 (空 = 随内容自适应) + 重置按钮;
/// 保存到 rfNode 显式尺寸并随位置持久化到后端 (graph slice setWidgetNodeSize)
function SizeEditor({ nodeId, kind }: { nodeId: string; kind: WidgetConfig['kind'] }) {
  const lang = useAppStore((s) => s.lang);
  const width = useAppStore((s) => s.rfNodes.find((n) => n.id === nodeId)?.width ?? null);
  const height = useAppStore((s) => s.rfNodes.find((n) => n.id === nodeId)?.height ?? null);
  const setWidgetNodeSize = useAppStore((s) => s.setWidgetNodeSize);
  const commitSize = (patch: { width?: number | null; height?: number | null }) => {
    const next = clampWidgetSize(kind, {
      width: patch.width !== undefined ? (patch.width ?? undefined) : (width ?? undefined),
      height: patch.height !== undefined ? (patch.height ?? undefined) : (height ?? undefined),
    });
    setWidgetNodeSize(nodeId, next);
  };
  const limits = widgetMinSize(kind);
  return (
    <section className="mt-3 pt-3 border-t border-border">
      <div className="text-[10px] font-semibold uppercase tracking-wider text-text-secondary mb-2">{t(lang, 'nodeSize')}</div>
      <div className="flex gap-2">
        <OptionalNumberField label={t(lang, 'nodeWidth')} value={width} placeholder={t(lang, 'sizeAuto')}
          onCommit={(w) => commitSize({ width: w })} />
        <OptionalNumberField label={t(lang, 'nodeHeight')} value={height} placeholder={t(lang, 'sizeAuto')}
          onCommit={(h) => commitSize({ height: h })} />
      </div>
      <div className="flex items-center justify-between">
        <span className="text-[10px] text-text-secondary">
          {t(lang, 'nodeSizeMinHint').replace('{w}', String(limits.minW)).replace('{h}', String(limits.minH))}
        </span>
        <button type="button" onClick={() => setWidgetNodeSize(nodeId, {})}
          className="inline-flex items-center gap-1 text-[10px] text-text-secondary hover:text-text-primary rounded px-1.5 py-1 hover:bg-bg-hover transition-colors">
          <RotateCcw size={10} /> {t(lang, 'resetSize')}
        </button>
      </div>
    </section>
  );
}

// ============ 自节点体内搬出的设置节 (FFT / Filter / Str / TextOut) ============
// 控件卡片只保留显示与值交互, 全部配置在这里编辑 (updateWidget → 图重编译)。

/// FFT 频域求解器参数 — 原节点内折叠面板的全部字段
function FftSettings({ widget, update }: {
  widget: Extract<WidgetConfig, { kind: 'FFT' }>;
  update: (next: Extract<WidgetConfig, { kind: 'FFT' }>) => void;
}) {
  const lang = useAppStore((s) => s.lang);
  const { windowSize, windowType, output, sampleRate } = widget.params;
  const patch = (p: Partial<typeof widget.params>) =>
    update({ kind: 'FFT', params: { ...widget.params, ...p } });
  return (
    <section className="mt-3 pt-3 border-t border-border">
      <div className="text-[10px] font-semibold uppercase tracking-wider text-text-secondary mb-2">{t(lang, 'fftSettings')}</div>
      <SelectField label={t(lang, 'spectrumWindowSize')} value={String(windowSize)}
        options={[256, 512, 1024, 2048, 4096].map((sz) => ({ value: String(sz), label: String(sz) }))}
        onChange={(v) => patch({ windowSize: Number(v) })} />
      <SelectField label={t(lang, 'spectrumWindowType')} value={windowType}
        options={([
          ['Rect', 'windowRect'],
          ['Hann', 'windowHann'],
          ['Hamming', 'windowHamming'],
          ['Blackman', 'windowBlackman'],
        ] as [WindowType, string][]).map(([value, key]) => ({ value, label: t(lang, key) }))}
        onChange={(v) => patch({ windowType: v as WindowType })} />
      <SelectField label={t(lang, 'spectrumOutputMode')} value={output}
        options={([
          ['Magnitude', 'spectrumMagnitude'],
          ['Power', 'spectrumPower'],
          ['PSD', 'spectrumPSD'],
          ['Decibel', 'spectrumDecibel'],
        ] as [SpectrumOutput, string][]).map(([value, key]) => ({ value, label: t(lang, key) }))}
        onChange={(v) => patch({ output: v as SpectrumOutput })} />
      <NumberField label={`${t(lang, 'filterSampleRate')} (Hz)`} value={sampleRate}
        onCommit={(v) => { if (v > 0) { patch({ sampleRate: v }); return true; } return false; }}
        error={t(lang, 'invalidStep')} />
    </section>
  );
}

/// 滤波器参数 — 原节点内折叠面板的全部字段
function FilterSettings({ widget, update }: {
  widget: Extract<WidgetConfig, { kind: 'Filter' }>;
  update: (next: Extract<WidgetConfig, { kind: 'Filter' }>) => void;
}) {
  const lang = useAppStore((s) => s.lang);
  const { preset, cutoff, low, high, sampleRate } = widget.params;
  const patch = (p: Partial<typeof widget.params>) =>
    update({ kind: 'Filter', params: { ...widget.params, ...p } });
  return (
    <section className="mt-3 pt-3 border-t border-border">
      <div className="text-[10px] font-semibold uppercase tracking-wider text-text-secondary mb-2">{t(lang, 'filterSettings')}</div>
      <SelectField label={t(lang, 'filterPreset')} value={preset}
        options={([
          ['Lowpass', 'filterLowpass'],
          ['Highpass', 'filterHighpass'],
          ['Bandpass', 'filterBandpass'],
          ['Bandstop', 'filterBandstop'],
        ] as [FilterPresetKind, string][]).map(([value, key]) => ({ value, label: t(lang, key) }))}
        onChange={(v) => patch({ preset: v as FilterPresetKind })} />
      {(preset === 'Lowpass' || preset === 'Highpass') && (
        <NumberField label={`${t(lang, 'filterCutoff')} (Hz)`} value={cutoff}
          onCommit={(v) => { if (v > 0) { patch({ cutoff: v }); return true; } return false; }}
          error={t(lang, 'invalidStep')} />
      )}
      {(preset === 'Bandpass' || preset === 'Bandstop') && (
        <>
          <NumberField label={`${t(lang, 'filterLow')} (Hz)`} value={low}
            onCommit={(v) => { if (v > 0) { patch({ low: v }); return true; } return false; }}
            error={t(lang, 'invalidStep')} />
          <NumberField label={`${t(lang, 'filterHigh')} (Hz)`} value={high}
            onCommit={(v) => { if (v > 0) { patch({ high: v }); return true; } return false; }}
            error={t(lang, 'invalidStep')} />
        </>
      )}
      <NumberField label={`${t(lang, 'filterSampleRate')} (Hz)`} value={sampleRate}
        onCommit={(v) => { if (v > 0) { patch({ sampleRate: v }); return true; } return false; }}
        error={t(lang, 'invalidStep')} />
    </section>
  );
}

/// 字符串操作参数 — tmpl 模板 (format) 与 pos/len/size 内联回退值。
/// 数值框是「端口未连接时的回退值」; 端口已连接时后端取上游值 (节点内原为禁用态, 面板中始终可编辑)。
function StrSettings({ widget, update }: {
  widget: Extract<WidgetConfig, { kind: 'Str' }>;
  update: (next: Extract<WidgetConfig, { kind: 'Str' }>) => void;
}) {
  const lang = useAppStore((s) => s.lang);
  const { id, op, tmpl, pos, len, size } = widget.params;
  const meta = STR_OP_PORTS[op];
  const patch = (p: Partial<typeof widget.params>) =>
    update({ kind: 'Str', params: { ...widget.params, ...p } });
  const INLINE_LABEL: Record<string, string> = { pos: 'strPortPos', len: 'strPortLen', size: 'strPortSize' };
  const VALUES = { pos, len, size } as const;
  return (
    <section className="mt-3 pt-3 border-t border-border">
      <div className="text-[10px] font-semibold uppercase tracking-wider text-text-secondary mb-2">{t(lang, 'strSettings')}</div>
      {op === 'format' && (
        <TextField label={t(lang, 'strFormatTmpl')} value={tmpl ?? ''}
          onCommit={(next) => patch({ tmpl: next })} />
      )}
      {meta.inlineNumPorts.map((portId) => (
        <NumberField key={portId} label={t(lang, INLINE_LABEL[portId] ?? portId)}
          value={VALUES[portId as 'pos' | 'len' | 'size']}
          onCommit={(v) => { if (v >= 0) { patch({ [portId]: Math.round(v) }); return true; } return false; }}
          error={t(lang, 'invalidStep')} />
      ))}
      <div className="text-[10px] text-text-secondary">{t(lang, 'strInlineFallbackHint')}</div>
      <div className="mt-1 text-[10px] text-text-muted break-all">id: {id}</div>
    </section>
  );
}

/// 文本下发参数 — 目标串口 / 换行 / 限速 (节点内只保留预览与发送按钮)
function TextOutSettings({ widget, update }: {
  widget: Extract<WidgetConfig, { kind: 'TextOut' }>;
  update: (next: Extract<WidgetConfig, { kind: 'TextOut' }>) => void;
}) {
  const lang = useAppStore((s) => s.lang);
  const nodes = useAppStore((s) => s.rfNodes);
  const { targetTransport, newline, minIntervalMs } = widget.params;
  const patch = (p: Partial<typeof widget.params>) =>
    update({ kind: 'TextOut', params: { ...widget.params, ...p } });
  const transports = nodes.filter((n) => n.type === 'transport');
  return (
    <section className="mt-3 pt-3 border-t border-border">
      <div className="text-[10px] font-semibold uppercase tracking-wider text-text-secondary mb-2">{t(lang, 'textOutSettings')}</div>
      <SelectField label={t(lang, 'textOutTarget')} value={targetTransport}
        options={[
          { value: '', label: t(lang, 'textOutNoTarget') },
          ...transports.map((n) => ({
            value: n.id,
            label: typeof n.data?.label === 'string' && n.data.label ? n.data.label : n.id,
          })),
        ]}
        onChange={(v) => patch({ targetTransport: v })} />
      <SelectField label={t(lang, 'textOutNewline')} value={newline}
        options={([
          ['none', 'textOutNlNone'],
          ['lf', 'textOutNlLf'],
          ['crlf', 'textOutNlCrlf'],
          ['cr', 'textOutNlCr'],
        ] as [typeof newline, string][]).map(([value, key]) => ({ value, label: t(lang, key) }))}
        onChange={(v) => patch({ newline: v as typeof newline })} />
      <NumberField label={`${t(lang, 'textOutInterval')} (ms)`} value={minIntervalMs}
        onCommit={(v) => { if (v >= 0) { patch({ minIntervalMs: Math.round(v) }); return true; } return false; }}
        error={t(lang, 'invalidStep')} />
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
    <>
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
      {widget.kind === 'TextInput' && <TextField label={t(lang, 'textInputPlaceholder')} value={widget.params.placeholder}
        onCommit={(placeholder) => update({ kind: 'TextInput', params: { ...widget.params, placeholder } })} />}
      {widget.kind === 'Custom' && <button type="button" className="w-full h-8 mt-2 bg-bg-button text-text-inverse rounded inline-flex items-center justify-center gap-1.5"
        onClick={() => openCustomEditor(widget.params.id)}><Code2 size={14} /> {t(lang, 'customWidgetEditor')}</button>}
      {widget.kind === 'FFT' && <FftSettings widget={widget} update={update} />}
      {widget.kind === 'Filter' && <FilterSettings widget={widget} update={update} />}
      {widget.kind === 'Str' && <StrSettings widget={widget} update={update} />}
      {widget.kind === 'TextOut' && <TextOutSettings widget={widget} update={update} />}
      <SizeEditor nodeId={widget.params.id} kind={widget.kind} />
    </>
  );
});
