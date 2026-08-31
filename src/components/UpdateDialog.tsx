//! 更新弹窗 — 自动检查发现新版本时弹出 (也可由状态栏指示器手动打开)
//!
//! 仿 AboutModal: 遮罩 + 居中卡片, Esc/点遮罩关闭
//! 下载中禁止关闭 (避免用户误以为更新已取消)

import { useEffect } from 'react';
import { X, RefreshCw } from 'lucide-react';
import { useAppStore } from '../store/appStore';
import { useUpdateStore } from '../store/updateStore';
import { t } from '../i18n';

export function UpdateDialog() {
  const lang = useAppStore((s) => s.lang);
  const status = useUpdateStore((s) => s.status);
  const updateInfo = useUpdateStore((s) => s.updateInfo);
  const currentVersion = useUpdateStore((s) => s.currentVersion);
  const progress = useUpdateStore((s) => s.progress);
  const error = useUpdateStore((s) => s.error);
  const downloadAndInstall = useUpdateStore((s) => s.downloadAndInstall);
  const relaunch = useUpdateStore((s) => s.relaunch);
  const skipVersion = useUpdateStore((s) => s.skipVersion);
  const closeDialog = useUpdateStore((s) => s.closeDialog);

  /// 下载中禁止关闭
  const closable = status !== 'downloading';

  useEffect(() => {
    if (!closable) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') closeDialog();
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [closable, closeDialog]);

  return (
    <div
      className="fixed inset-0 bg-bg-overlay flex items-center justify-center z-modal animate-[settings-fade-in_0.15s_ease-out]"
      onClick={(event) => { if (event.target === event.currentTarget && closable) closeDialog(); }}
      onKeyDown={(event) => { if (closable && (event.key === 'Enter' || event.key === ' ')) { event.preventDefault(); closeDialog(); } }}
      role="button"
      tabIndex={0}
    >
      <div
        className="w-[440px] max-w-[90vw] max-h-[80vh] bg-bg-sidebar border border-border rounded-lg shadow-modal py-6 px-7 flex flex-col gap-2 relative animate-[settings-slide-in_0.2s_ease-out]"
        role="dialog"
        aria-modal="true"
      >
        {closable && (
          <button
            className="absolute top-2 right-2 w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-bright transition-colors cursor-pointer bg-transparent border-none"
            onClick={closeDialog}
            title={t(lang, 'updateLater')}
          >
            <X size={16} />
          </button>
        )}

        <h2 className="text-base font-semibold text-text-bright m-0">
          {t(lang, 'updateAvailableTitle')}
        </h2>

        {updateInfo && (
          <p className="text-sm text-text-secondary m-0">
            <code className="bg-bg-input px-1.5 py-0.5 rounded-sm text-text-primary font-mono">v{currentVersion}</code>
            <span className="mx-1.5">→</span>
            <code className="bg-bg-input px-1.5 py-0.5 rounded-sm text-text-primary font-mono">v{updateInfo.version}</code>
          </p>
        )}
        {updateInfo?.date && (
          <p className="text-xs text-text-secondary m-0">
            {t(lang, 'updateReleaseDate')}: {updateInfo.date}
          </p>
        )}

        {updateInfo?.notes && (
          <>
            <div className="text-xs font-semibold uppercase tracking-[0.5px] text-text-secondary mt-2">
              {t(lang, 'updateReleaseNotes')}
            </div>
            <div className="text-xs text-text-primary leading-relaxed whitespace-pre-wrap max-h-[240px] overflow-y-auto bg-bg-input rounded px-3 py-2 border border-border">
              {updateInfo.notes}
            </div>
          </>
        )}

        {status === 'downloading' && (
          <div className="flex items-center gap-2 mt-3">
            <div className="flex-1 h-1.5 rounded-full bg-bg-hover overflow-hidden">
              <div
                className="h-full bg-accent transition-all duration-200"
                style={{ width: `${progress}%` }}
              />
            </div>
            <span className="text-xs text-text-secondary tabular-nums">{progress}%</span>
          </div>
        )}

        {status === 'ready' && (
          <p className="text-xs text-text-secondary m-0 mt-2">{t(lang, 'updateReadyHint')}</p>
        )}

        {status === 'error' && (
          <p className="text-xs text-red-400 m-0 mt-2 break-all">
            {t(lang, 'updateError')}: {error}
          </p>
        )}

        <div className="flex justify-end gap-2 mt-4">
          {status === 'available' && (
            <>
              <button
                className="bg-transparent text-text-secondary border-none px-2.5 py-1 text-xs cursor-pointer rounded transition-colors hover:bg-bg-hover hover:text-text-primary"
                onClick={skipVersion}
              >
                {t(lang, 'updateSkipVersion')}
              </button>
              <button
                className="bg-transparent text-text-primary border border-border px-2.5 py-1 text-xs cursor-pointer rounded transition-all hover:bg-bg-hover hover:border-accent hover:text-text-bright"
                onClick={closeDialog}
              >
                {t(lang, 'updateLater')}
              </button>
              <button
                className="px-3 py-1 bg-bg-button text-text-inverse border-none rounded cursor-pointer text-xs inline-flex items-center gap-1.5 transition-colors hover:bg-bg-button-hover"
                onClick={() => void downloadAndInstall()}
              >
                {t(lang, 'updateNow')}
              </button>
            </>
          )}
          {status === 'downloading' && (
            <span className="text-xs text-text-secondary inline-flex items-center gap-1.5">
              <RefreshCw size={12} className="animate-spin" />
              {t(lang, 'updateDownloading')}
            </span>
          )}
          {status === 'ready' && (
            <button
              className="px-3 py-1 bg-bg-button text-text-inverse border-none rounded cursor-pointer text-xs transition-colors hover:bg-bg-button-hover"
              onClick={() => void relaunch()}
            >
              {t(lang, 'updateRestart')}
            </button>
          )}
          {status === 'error' && (
            <button
              className="px-3 py-1 bg-bg-button text-text-inverse border-none rounded cursor-pointer text-xs transition-colors hover:bg-bg-button-hover"
              onClick={() => void downloadAndInstall()}
            >
              {t(lang, 'updateRetry')}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
