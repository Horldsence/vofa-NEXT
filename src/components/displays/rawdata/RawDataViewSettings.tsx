import { Palette, PanelRight, AlignLeft } from 'lucide-react';
import { t, type Lang } from '../../../i18n';
import type { HexColorMode, SendPanelMode } from './rawDataViewHelpers';

interface Props {
  hexColorMode: HexColorMode;
  sendPanelMode: SendPanelMode;
  showTimestamp: boolean;
  showOffset: boolean;
  onHexColorModeChange: (v: HexColorMode) => void;
  onSendPanelModeChange: (v: SendPanelMode) => void;
  onShowTimestampChange: (v: boolean) => void;
  onShowOffsetChange: (v: boolean) => void;
  lang: Lang;
}

export function RawDataViewSettings({
  hexColorMode,
  sendPanelMode,
  showTimestamp,
  showOffset,
  onHexColorModeChange,
  onSendPanelModeChange,
  onShowTimestampChange,
  onShowOffsetChange,
  lang,
}: Props) {
  const hexColorOptions: { mode: HexColorMode; label: string }[] = [
    { mode: 'none', label: t(lang, 'hexColorNone') },
    { mode: 'printable', label: t(lang, 'hexColorPrintable') },
    { mode: 'range', label: t(lang, 'hexColorRange') },
  ];

  const sendPanelOptions: { mode: SendPanelMode; label: string }[] = [
    { mode: 'bottom', label: t(lang, 'sendPanelBottom') },
    { mode: 'separate', label: t(lang, 'sendPanelSeparate') },
  ];

  return (
    <div className="flex flex-col gap-4">
      <div>
        <h4 className="text-xs font-semibold text-text-secondary mb-2 flex items-center gap-1">
          <Palette size={12} /> {t(lang, 'hexColorMode')}
        </h4>
        <div className="flex flex-col gap-1">
          {hexColorOptions.map((opt) => (
            <button
              key={opt.mode}
              className={`text-left px-2 py-1 rounded text-xs transition-colors ${hexColorMode === opt.mode ? 'bg-bg-active text-text-bright' : 'text-text-secondary hover:bg-bg-hover hover:text-text-primary'}`}
              onClick={() => onHexColorModeChange(opt.mode)}
            >
              {opt.label}
            </button>
          ))}
        </div>
      </div>
      <div>
        <h4 className="text-xs font-semibold text-text-secondary mb-2 flex items-center gap-1">
          <PanelRight size={12} /> {t(lang, 'sendPanelMode')}
        </h4>
        <div className="flex flex-col gap-1">
          {sendPanelOptions.map((opt) => (
            <button
              key={opt.mode}
              className={`text-left px-2 py-1 rounded text-xs transition-colors ${sendPanelMode === opt.mode ? 'bg-bg-active text-text-bright' : 'text-text-secondary hover:bg-bg-hover hover:text-text-primary'}`}
              onClick={() => onSendPanelModeChange(opt.mode)}
            >
              {opt.label}
            </button>
          ))}
        </div>
      </div>
      <div>
        <h4 className="text-xs font-semibold text-text-secondary mb-2 flex items-center gap-1">
          <AlignLeft size={12} /> {t(lang, 'displayOptions')}
        </h4>
        <label className="flex items-center gap-2 text-xs text-text-secondary hover:text-text-primary cursor-pointer mb-1.5">
          <input
            type="checkbox"
            checked={showTimestamp}
            onChange={(e) => onShowTimestampChange(e.target.checked)}
            className="accent-accent"
          />
          {t(lang, 'showTimestamp')}
        </label>
        <label className="flex items-center gap-2 text-xs text-text-secondary hover:text-text-primary cursor-pointer">
          <input
            type="checkbox"
            checked={showOffset}
            onChange={(e) => onShowOffsetChange(e.target.checked)}
            className="accent-accent"
          />
          {t(lang, 'showOffset')}
        </label>
      </div>
    </div>
  );
}
