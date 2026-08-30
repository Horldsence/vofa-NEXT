//! 钥匙串授权拒绝说明 — 仅由启动读取返回 AiKeyringAccessDenied 时显示。

import { useState } from 'react';
import { KeyRound, RefreshCw, ShieldCheck } from 'lucide-react';
import { t } from '../i18n';
import { useAppStore } from '../store/appStore';
import { useSettingsStore } from '../store/settingsStore';

export function KeychainPermissionDialog() {
  const lang = useAppStore((state) => state.lang);
  const retrying = useSettingsStore((state) => state.keychainPermissionRetrying);
  const retryError = useSettingsStore((state) => state.keychainPermissionRetryError);
  const dismiss = useSettingsStore((state) => state.dismissKeychainPermissionPrompt);
  const retry = useSettingsStore((state) => state.retryKeychainPermission);
  const [dontRemind, setDontRemind] = useState(false);

  return (
    <div className="fixed inset-0 bg-bg-overlay flex items-center justify-center z-modal animate-[settings-fade-in_0.15s_ease-out]">
      <div
        className="w-[460px] max-w-[92vw] bg-bg-sidebar border border-border rounded-lg shadow-modal px-7 py-6 flex flex-col gap-4 animate-[settings-slide-in_0.2s_ease-out]"
        role="dialog"
        aria-modal="true"
        aria-labelledby="keychain-permission-title"
      >
        <div className="flex items-start gap-3">
          <div className="w-9 h-9 shrink-0 rounded-lg bg-accent/15 text-accent flex items-center justify-center">
            <KeyRound size={20} aria-hidden="true" />
          </div>
          <div className="min-w-0">
            <h2
              id="keychain-permission-title"
              className="text-base font-semibold text-text-bright m-0"
            >
              {t(lang, 'keychainPermissionTitle')}
            </h2>
            <p className="text-sm text-text-secondary leading-relaxed m-0 mt-1">
              {t(lang, 'keychainPermissionDescription')}
            </p>
          </div>
        </div>

        <div className="rounded-md border border-border bg-bg-input px-3 py-2.5 flex gap-2.5">
          <ShieldCheck size={17} className="text-accent shrink-0 mt-0.5" aria-hidden="true" />
          <div className="text-xs text-text-secondary leading-relaxed">
            <p className="m-0">{t(lang, 'keychainPermissionPurpose')}</p>
            <p className="m-0 mt-1">{t(lang, 'keychainPermissionImpact')}</p>
          </div>
        </div>

        <p className="text-xs text-text-secondary leading-relaxed m-0">
          {t(lang, 'keychainPermissionRetryHint')}
        </p>

        {retryError && (
          <p className="text-xs text-red-400 leading-relaxed m-0" role="alert">
            {t(
              lang,
              retryError === 'denied'
                ? 'keychainPermissionDeniedAgain'
                : 'keychainPermissionRetryFailed'
            )}
          </p>
        )}

        <label className="inline-flex items-center gap-2 text-xs text-text-secondary cursor-pointer w-fit">
          <input
            type="checkbox"
            className="accent-accent"
            checked={dontRemind}
            disabled={retrying}
            onChange={(event) => setDontRemind(event.target.checked)}
          />
          {t(lang, 'keychainPermissionDontRemind')}
        </label>

        <div className="flex justify-end gap-2">
          <button
            type="button"
            className="bg-transparent text-text-primary border border-border px-3 py-1.5 text-xs cursor-pointer rounded transition-all hover:bg-bg-hover hover:border-accent hover:text-text-bright disabled:opacity-50 disabled:cursor-not-allowed"
            disabled={retrying}
            onClick={() => dismiss(dontRemind)}
          >
            {t(lang, 'keychainPermissionLater')}
          </button>
          <button
            type="button"
            className="px-3 py-1.5 bg-bg-button text-text-inverse border-none rounded cursor-pointer text-xs inline-flex items-center gap-1.5 transition-colors hover:bg-bg-button-hover disabled:opacity-60 disabled:cursor-wait"
            disabled={retrying}
            onClick={() => void retry()}
          >
            {retrying && <RefreshCw size={12} className="animate-spin" aria-hidden="true" />}
            {t(
              lang,
              retrying ? 'keychainPermissionRequesting' : 'keychainPermissionRequestAgain'
            )}
          </button>
        </div>
      </div>
    </div>
  );
}
