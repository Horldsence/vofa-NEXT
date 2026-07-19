import { t } from '../../../i18n';
import type { Lang } from '../../../i18n';
import type { UdpConfig } from '../../../types';

interface UdpFormProps {
  params: UdpConfig;
  onChange: (patch: Partial<UdpConfig>) => void;
  lang: Lang;
}

const inputClass = 'form-input';

export function UdpForm({ params, onChange, lang }: UdpFormProps) {
  return (
    <>
      <div className="flex gap-2">
        <div className="mb-2.5 flex-1">
          <label className="block text-xs text-text-secondary mb-1">{t(lang, 'localAddr')}</label>
          <input
            type="text"
            value={params.local_addr}
            onChange={(e) => onChange({ local_addr: e.target.value })}
            className={inputClass}
          />
        </div>
        <div className="mb-2.5 w-20">
          <label className="block text-xs text-text-secondary mb-1">{t(lang, 'localPort')}</label>
          <input
            type="number"
            value={params.local_port}
            onChange={(e) => onChange({ local_port: parseInt(e.target.value) || 0 })}
            className={inputClass}
          />
        </div>
      </div>
      <div className="flex gap-2">
        <div className="mb-2.5 flex-1">
          <label className="block text-xs text-text-secondary mb-1">{t(lang, 'remoteAddr')}</label>
          <input
            type="text"
            value={params.remote_addr}
            onChange={(e) => onChange({ remote_addr: e.target.value })}
            className={inputClass}
          />
        </div>
        <div className="mb-2.5 w-20">
          <label className="block text-xs text-text-secondary mb-1">{t(lang, 'remotePort')}</label>
          <input
            type="number"
            value={params.remote_port}
            onChange={(e) => onChange({ remote_port: parseInt(e.target.value) || 0 })}
            className={inputClass}
          />
        </div>
      </div>
    </>
  );
}
