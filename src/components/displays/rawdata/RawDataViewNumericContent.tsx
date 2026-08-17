import { t } from '../../../i18n';
import type { Lang } from '../../../i18n';
import { formatTime } from './rawDataViewHelpers';

interface Props {
  numRows: Array<{ seq: number; ts: number; value: number }>;
  showTimestamp: boolean;
  lang: Lang;
  grouping: string;
  repr: string;
  channel: string;
}

export function RawDataViewNumericContent({
  numRows,
  showTimestamp,
  lang,
}: Props) {
  return (
    <div className="flex-1 flex flex-col min-h-0 overflow-hidden font-mono animate-rawdata-enter select-text">
      <div className="flex-1 overflow-auto min-h-0">
        {numRows.length === 0 ? (
          <div className="flex items-center justify-center h-32 text-text-secondary text-sm">
            {t(lang, 'rawDataEmpty')}
          </div>
        ) : (
          numRows.map((r) => (
            <div key={r.seq} className="flex items-center gap-2 px-2 text-xs font-mono animate-rawdata-row">
              {showTimestamp && (
                <span className="text-accent min-w-[92px] text-right">{formatTime(r.ts)}</span>
              )}
              <span className="text-text-primary">
                {Number.isInteger(r.value) ? r.value.toFixed(0) : r.value.toFixed(4)}
              </span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
