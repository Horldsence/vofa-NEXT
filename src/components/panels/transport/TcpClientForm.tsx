import { t } from '../../../i18n';
import type { Lang } from '../../../i18n';
import type { TcpClientConfig } from '../../../types';

interface TcpClientFormProps {
  params: TcpClientConfig;
  onChange: (patch: Partial<TcpClientConfig>) => void;
  lang: Lang;
}

const inputClass = 'form-input';

export function TcpClientForm({ params, onChange, lang }: TcpClientFormProps) {
  return (
    <div className="flex gap-2">
      <div className="mb-2.5 flex-1">
        <label className="block text-xs text-text-secondary mb-1">{t(lang, 'host')}</label>
        <input
          type="text"
          value={params.host}
          onChange={(e) => onChange({ host: e.target.value })}
          className={inputClass}
        />
      </div>
      <div className="mb-2.5 w-20">
        <label className="block text-xs text-text-secondary mb-1">{t(lang, 'port')}</label>
        <input
          type="number"
          value={params.port}
          onChange={(e) => onChange({ port: parseInt(e.target.value) || 0 })}
          className={inputClass}
        />
      </div>
    </div>
  );
}
