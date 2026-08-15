//! 数据丢弃说明弹层 — 状态栏丢弃告警 / 原始数据丢弃徽标共用
//!
//! - variant='rawdata': 指引到 设置→数据缓存 (原始数据缓冲容量)
//! - variant='pipeline': 指引到 设置→性能 (并行 worker / 缓冲项)
//! - ESC / 点击遮罩关闭

import { useEffect } from 'react';
import { X, Settings } from 'lucide-react';
import { useAppStore } from '../../store/appStore';
import { useSettingsStore } from '../../store/settingsStore';
import { t } from '../../i18n';

interface DroppedInfoPopoverProps {
  open: boolean;
  onClose: () => void;
  variant: 'rawdata' | 'pipeline';
}

export function DroppedInfoPopover({ open, onClose, variant }: DroppedInfoPopoverProps) {
  const lang = useAppStore((s) => s.lang);
  const openSettings = useSettingsStore((s) => s.open);

  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [open, onClose]);

  if (!open) return null;

  const isRawdata = variant === 'rawdata';
  const titleKey = isRawdata ? 'droppedInfoTitleRawdata' : 'droppedInfoTitle';
  const whatKey = isRawdata ? 'droppedInfoWhatRawdata' : 'droppedInfoWhatPipeline';
  const howKey = isRawdata ? 'droppedInfoHowRawdata' : 'droppedInfoHowPipeline';

  return (
    <div
      className="fixed inset-0 bg-bg-overlay flex items-center justify-center z-modal animate-[settings-fade-in_0.15s_ease-out]"
      onClick={onClose}
    >
      <div
        className="w-[420px] max-w-[90vw] bg-bg-sidebar border border-border rounded-lg shadow-modal p-6 flex flex-col gap-3 relative animate-[settings-slide-in_0.2s_ease-out]"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
      >
        <button
          className="absolute top-2 right-2 w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-bright transition-colors cursor-pointer bg-transparent border-none"
          onClick={onClose}
          title={t(lang, 'settingsClose')}
        >
          <X size={16} />
        </button>

        <h2 className="text-base font-semibold text-text-bright m-0">
          {t(lang, titleKey)}
        </h2>
        <p className="text-sm text-text-primary leading-relaxed m-0">
          {t(lang, whatKey)}
        </p>
        <p className="text-sm text-text-primary leading-relaxed m-0">
          {t(lang, 'droppedInfoWhy')}
        </p>
        <p className="text-sm text-text-primary leading-relaxed m-0">
          {t(lang, howKey)}
        </p>

        <div className="flex justify-end mt-1">
          <button
            className="flex items-center gap-1.5 px-3 py-1.5 rounded bg-bg-button text-text-inverse text-sm hover:opacity-90 transition-opacity cursor-pointer border-none"
            onClick={() => {
              onClose();
              openSettings(variant === 'rawdata' ? 'data' : 'performance');
            }}
          >
            <Settings size={14} />
            {t(lang, 'droppedInfoOpenSettings')}
          </button>
        </div>
      </div>
    </div>
  );
}
