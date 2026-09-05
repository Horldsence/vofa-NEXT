import type { Node } from '@xyflow/react';
import { useAppStore } from '../../../store/appStore';
import type { WidgetBinding, WidgetConfig } from '../../../types';
import { t } from '../../../i18n';
import { NumberField, TextField } from '../../ui/fields';

export type BindableKind = 'Knob' | 'Slider' | 'Button' | 'Radio' | 'Checkbox';
type AnyBindableWidget = Extract<WidgetConfig, { kind: BindableKind }>;

function nodeDisplayName(node: Node): string {
  return typeof node.data.label === 'string' ? node.data.label : node.id;
}

interface BindingSectionProps<K extends BindableKind> {
  widget: Extract<WidgetConfig, { kind: K }>;
  update: (widget: Extract<WidgetConfig, { kind: K }>) => void;
}

/// 传输绑定编辑节 — 输入控件的值下发通道 (None / Auto 协议编码 / Manual 模板)。
/// 泛型保持调用方的 kind 收窄; 内部以具体 union 操作, update 时还原泛型。
export function BindingSection<K extends BindableKind>({ widget, update }: BindingSectionProps<K>) {
  const lang = useAppStore((s) => s.lang);
  const nodes = useAppStore((s) => s.rfNodes);
  const edges = useAppStore((s) => s.rfEdges);
  const w = widget as AnyBindableWidget;
  const binding = w.params.binding;
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
  const setBinding = (next: WidgetBinding) =>
    update({ ...w, params: { ...w.params, binding: next } } as Extract<WidgetConfig, { kind: K }>);

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
