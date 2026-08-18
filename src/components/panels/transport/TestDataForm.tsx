import { Info } from 'lucide-react';
import { t } from '../../../i18n';
import type { Lang } from '../../../i18n';
import type { TestDataConfig } from '../../../types';

interface TestDataFormProps {
  params: TestDataConfig;
  onChange: (patch: Partial<TestDataConfig>) => void;
  lang: Lang;
  /// 当前关联协议引擎的显示名 (由属性面板按字节边下游 Protocol 节点解析)
  protocolLabel?: string;
}

const inputClass = 'form-input';
const selectClass = 'form-select';
const signalLabels: { value: TestDataConfig['signal']; key: string }[] = [
  { value: 'Sine', key: 'sine' },
  { value: 'Square', key: 'square' },
  { value: 'Triangle', key: 'triangle' },
  { value: 'Sawtooth', key: 'sawtooth' },
  { value: 'Random', key: 'random' },
  { value: 'Dc', key: 'dc' },
  { value: 'Chirp', key: 'chirp' },
  { value: 'Steps', key: 'steps' },
  { value: 'Noise', key: 'noise' },
  { value: 'MultiTone', key: 'multitone' },
];

export function TestDataForm({ params, onChange, lang, protocolLabel }: TestDataFormProps) {
  const protocol = protocolLabel ?? 'JustFloat';

  return (
    <>
      <div className="flex gap-2">
        <div className="mb-2.5 flex-1">
          <label className="block text-xs text-text-secondary mb-1">{t(lang, 'channels')}</label>
          <input
            type="number"
            min={1}
            max={32}
            value={params.channels}
            onChange={(e) => onChange({ channels: parseInt(e.target.value) || 1 })}
            className={inputClass}
          />
        </div>
        <div className="mb-2.5 flex-1">
          <label className="block text-xs text-text-secondary mb-1">{t(lang, 'sampleRate')}</label>
          <input
            type="number"
            min={1}
            max={10000}
            value={params.sample_rate}
            onChange={(e) => onChange({ sample_rate: parseInt(e.target.value) || 1 })}
            className={inputClass}
          />
        </div>
      </div>
      <div className="mb-2.5">
        <label className="block text-xs text-text-secondary mb-1">{t(lang, 'signalType')}</label>
        <select
          value={params.signal}
          onChange={(e) => onChange({ signal: e.target.value as TestDataConfig['signal'] })}
          className={selectClass}
        >
          {signalLabels.map((opt) => (
            <option key={opt.value} value={opt.value}>{t(lang, opt.key)}</option>
          ))}
        </select>
      </div>
      <div className="p-2 bg-bg-input rounded text-xs text-text-secondary leading-relaxed flex gap-2 mb-1">
        <Info size={14} className="flex-shrink-0 mt-0.25" />
        <span>
          {lang === 'zh'
            ? `测试数据将根据当前协议引擎 (${protocol}) 自动适配数据格式。`
            : `Test data will be auto-formatted for the current protocol engine (${protocol}).`}
        </span>
      </div>
    </>
  );
}
