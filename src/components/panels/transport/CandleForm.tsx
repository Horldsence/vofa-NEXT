import { RefreshCw, Info } from 'lucide-react';
import { t } from '../../../i18n';
import type { Lang } from '../../../i18n';
import type { CandleConfig, CanBitrate, CandleDeviceInfo } from '../../../types';

interface CandleFormProps {
  params: CandleConfig;
  onChange: (patch: Partial<CandleConfig>) => void;
  lang: Lang;
  candleDevices: CandleDeviceInfo[];
  candleLoading: boolean;
  refreshCandleDevices: () => void;
}

const selectClass = 'form-select';
const canBitrateOptions: { value: CanBitrate; label: string }[] = [
  { value: 'bps100k', label: '100k' },
  { value: 'bps125k', label: '125k' },
  { value: 'bps250k', label: '250k' },
  { value: 'bps500k', label: '500k' },
  { value: 'bps1m', label: '1M' },
];

export function CandleForm({ params, onChange, lang, candleDevices, candleLoading, refreshCandleDevices }: CandleFormProps) {
  return (
    <>
      <div className="mb-2.5">
        <div className="flex items-center justify-between mb-1">
          <label className="block text-xs text-text-secondary">{t(lang, 'candleDevice')}</label>
          <button
            type="button"
            onClick={() => refreshCandleDevices()}
            disabled={candleLoading}
            className="text-text-secondary hover:text-text-primary transition-colors disabled:opacity-50 cursor-pointer"
            title={t(lang, 'refresh')}
          >
            <RefreshCw size={12} className={candleLoading ? 'animate-spin' : ''} />
          </button>
        </div>
        <select
          value={`${params.bus}:${params.address}`}
          onChange={(e) => {
            const sel = e.target.value;
            const dev = candleDevices.find((d) => `${d.bus}:${d.address}` === sel);
            if (dev) {
              onChange({ bus: dev.bus, address: dev.address });
            }
          }}
          className={selectClass}
        >
          {candleDevices.length === 0 && (
            <option value="">-- {t(lang, 'noCandleDevices')} --</option>
          )}
          {candleDevices.map((d) => (
            <option key={`${d.bus}:${d.address}`} value={`${d.bus}:${d.address}`}>
              Bus {d.bus}:Dev {d.address} ({d.vid.toString(16).padStart(4, '0').toUpperCase()}:{d.pid.toString(16).padStart(4, '0').toUpperCase()})
              {d.product ? ` - ${d.product}` : ''}
            </option>
          ))}
        </select>
      </div>
      <div className="flex gap-2">
        <div className="mb-2.5 flex-1">
          <label className="block text-xs text-text-secondary mb-1">{t(lang, 'channel')}</label>
          <select
            value={params.channel}
            onChange={(e) => onChange({ channel: parseInt(e.target.value) || 0 })}
            className={selectClass}
          >
            <option value={0}>0</option>
            <option value={1}>1</option>
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
        <span>{t(lang, 'candleLightDesc')}</span>
      </div>
    </>
  );
}
