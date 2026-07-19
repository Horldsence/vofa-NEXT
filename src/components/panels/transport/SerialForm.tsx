import { PortPicker } from '../PortPicker';
import { t } from '../../../i18n';
import type { Lang } from '../../../i18n';
import type { SerialConfig } from '../../../types';

interface SerialFormProps {
  params: SerialConfig;
  onChange: <K extends keyof SerialConfig>(key: K, value: SerialConfig[K]) => void;
  lang: Lang;
}

const baudRates = [9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600];
const selectClass = 'form-select';

export function SerialForm({ params, onChange, lang }: SerialFormProps) {
  return (
    <>
      <PortPicker />
      <div className="mb-2.5">
        <label className="block text-xs text-text-secondary mb-1">{t(lang, 'baudRate')}</label>
        <select
          className={selectClass}
          value={params.baud_rate}
          onChange={(e) => onChange('baud_rate', parseInt(e.target.value))}
        >
          {baudRates.map((rate) => (
            <option key={rate} value={rate}>{rate}</option>
          ))}
        </select>
      </div>
      <div className="flex gap-2">
        <div className="mb-2.5 flex-1">
          <label className="block text-xs text-text-secondary mb-1">{t(lang, 'dataBits')}</label>
          <select
            className={selectClass}
            value={params.data_bits}
            onChange={(e) => onChange('data_bits', parseInt(e.target.value))}
          >
            {[5, 6, 7, 8].map((b) => <option key={b} value={b}>{b}</option>)}
          </select>
        </div>
        <div className="mb-2.5 flex-1">
          <label className="block text-xs text-text-secondary mb-1">{t(lang, 'parity')}</label>
          <select
            className={selectClass}
            value={params.parity}
            onChange={(e) => onChange('parity', e.target.value as SerialConfig['parity'])}
          >
            <option value="none">None</option>
            <option value="odd">Odd</option>
            <option value="even">Even</option>
          </select>
        </div>
      </div>
      <div className="flex gap-2">
        <div className="mb-2.5 flex-1">
          <label className="block text-xs text-text-secondary mb-1">{t(lang, 'stopBits')}</label>
          <select
            className={selectClass}
            value={params.stop_bits}
            onChange={(e) => onChange('stop_bits', e.target.value as SerialConfig['stop_bits'])}
          >
            <option value="one">1</option>
            <option value="two">2</option>
          </select>
        </div>
        <div className="mb-2.5 flex-1">
          <label className="block text-xs text-text-secondary mb-1">{t(lang, 'flowControl')}</label>
          <select
            className={selectClass}
            value={params.flow_control}
            onChange={(e) => onChange('flow_control', e.target.value as SerialConfig['flow_control'])}
          >
            <option value="none">None</option>
            <option value="software">Software</option>
            <option value="hardware">Hardware</option>
          </select>
        </div>
      </div>
    </>
  );
}
