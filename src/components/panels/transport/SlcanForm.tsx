import { Info } from 'lucide-react';
import { PortPicker } from '../PortPicker';
import { t } from '../../../i18n';
import type { Lang } from '../../../i18n';
import type { SlcanConfig, CanBitrate } from '../../../types';

interface SlcanFormProps {
  params: SlcanConfig;
  onChange: (patch: Partial<SlcanConfig>) => void;
  lang: Lang;
}

const selectClass = 'form-select';
const slcanBaudOptions = [9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600];
const canBitrateOptions: { value: CanBitrate; label: string }[] = [
  { value: 'bps100k', label: '100k' },
  { value: 'bps125k', label: '125k' },
  { value: 'bps250k', label: '250k' },
  { value: 'bps500k', label: '500k' },
  { value: 'bps1m', label: '1M' },
];

export function SlcanForm({ params, onChange, lang }: SlcanFormProps) {
  return (
    <>
      <PortPicker />
      <div className="flex gap-2">
        <div className="mb-2.5 flex-1">
          <label className="block text-xs text-text-secondary mb-1">{t(lang, 'baudRate')}</label>
          <select
            value={params.baud_rate}
            onChange={(e) => onChange({ baud_rate: parseInt(e.target.value) || 115200 })}
            className={selectClass}
          >
            {slcanBaudOptions.map((b) => <option key={b} value={b}>{b}</option>)}
          </select>
        </div>
        <div className="mb-2.5 flex-1">
          <label className="block text-xs text-text-secondary mb-1">{t(lang, 'canBitrate')}</label>
          <select
            value={params.can_bitrate}
            onChange={(e) => onChange({ can_bitrate: e.target.value as CanBitrate })}
            className={selectClass}
          >
            {canBitrateOptions.map((opt) => (
              <option key={opt.value} value={opt.value}>{opt.label}</option>
            ))}
          </select>
        </div>
      </div>
      <div className="p-2 bg-bg-input rounded text-xs text-text-secondary leading-relaxed flex gap-2 mb-1">
        <Info size={14} className="flex-shrink-0 mt-0.25" />
        <span>{t(lang, 'slcanDesc')}</span>
      </div>
    </>
  );
}
