import { t } from '../../../i18n';
import type { Lang } from '../../../i18n';
import type { TcpServerConfig } from '../../../types';

interface TcpServerFormProps {
  params: TcpServerConfig;
  onChange: (patch: Partial<TcpServerConfig>) => void;
  lang: Lang;
}

const inputClass = 'form-input';

export function TcpServerForm({ params, onChange, lang }: TcpServerFormProps) {
  return (
    <div className="flex gap-2">
      <div className="mb-2.5 flex-1">
        <label className="block text-xs text-text-secondary mb-1">{t(lang, 'listenAddr')}</label>
        <input
          type="text"
          value={params.listen_addr}
          onChange={(e) => onChange({ listen_addr: e.target.value })}
          className={inputClass}
        />
      </div>
      <div className="mb-2.5 w-20">
        <label className="block text-xs text-text-secondary mb-1">{t(lang, 'listenPort')}</label>
        <input
          type="number"
          value={params.listen_port}
          onChange={(e) => onChange({ listen_port: parseInt(e.target.value) || 0 })}
          className={inputClass}
        />
      </div>
    </div>
  );
}
