import { useMemo } from 'react';
import { t } from '../../i18n';

export interface LiveModePanelProps {
  portNames: string[];
  liveOutputs: Record<string, number>;
  enableValid: boolean;
  enableFrameCount: boolean;
  enableLastTimestamp: boolean;
  enableFps: boolean;
  lang: ReturnType<typeof import('../../store/appStore').useAppStore.getState>['lang'];
}

export function LiveModePanel({ portNames, liveOutputs, enableValid, enableFrameCount, enableLastTimestamp, enableFps, lang }: LiveModePanelProps) {
  const allPorts = useMemo(() => {
    const ports = [...portNames];
    if (enableValid) ports.push('valid');
    if (enableFrameCount) ports.push('frame_count');
    if (enableLastTimestamp) ports.push('last_timestamp');
    if (enableFps) ports.push('fps');
    return ports;
  }, [portNames, enableValid, enableFrameCount, enableLastTimestamp, enableFps]);

  return (
    <>
      <div className="text-[10px] text-text-secondary uppercase tracking-wide font-semibold">{t(lang, 'fdLiveOutputs')}</div>
      <div className="bg-bg-editor border border-border rounded p-2 flex flex-col gap-1">
        {allPorts.length === 0 ? (
          <div className="text-xs text-text-secondary opacity-60 italic py-2 text-center">{t(lang, 'fdNoPorts')}</div>
        ) : (
          allPorts.map((port) => {
            const val = liveOutputs[port];
            const hasVal = typeof val === 'number';
            return (
              <div key={port} className="flex items-center justify-between gap-2 px-1.5 py-0.5 bg-bg-editor rounded-sm">
                <span className="text-[10px] text-text-secondary font-mono">{port}</span>
                <span className={`text-xs font-mono ${hasVal ? 'text-green' : 'text-text-secondary opacity-60'}`}>
                  {hasVal ? val.toFixed(4) : '—'}
                </span>
              </div>
            );
          })
        )}
      </div>
      <div className="text-[10px] text-text-secondary opacity-70 px-1">
        {t(lang, 'fdLiveHint')}
      </div>
    </>
  );
}
