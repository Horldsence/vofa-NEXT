//! 关于对话框 — 从菜单栏 / Activity Bar 触发

import { useEffect, useState } from 'react';
import { X, Github, ExternalLink } from 'lucide-react';
import { getName, getVersion } from '@tauri-apps/api/app';
import { openUrl } from '@tauri-apps/plugin-opener';
import { useAppStore } from '../store/appStore';
import { t } from '../i18n';

interface AboutModalProps {
  isOpen: boolean;
  onClose: () => void;
}

const APP_AUTHOR = 'peng';
const APP_LICENSE = 'MIT';
const APP_GITHUB = 'https://github.com/pengheng/vofa-next';
const APP_DOCS = 'https://github.com/pengheng/vofa-next#readme';

export function AboutModal({ isOpen, onClose }: AboutModalProps) {
  const lang = useAppStore((s) => s.lang);
  const [appName, setAppName] = useState('VOFA-Next');
  const [appVersion, setAppVersion] = useState('0.0.0');

  useEffect(() => {
    if (!isOpen) return;
    void getName().then(setAppName).catch(() => setAppName('VOFA-Next'));
    void getVersion().then(setAppVersion).catch(() => setAppVersion('0.0.0'));
  }, [isOpen]);

  useEffect(() => {
    if (!isOpen) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  return (
    <div
      className="fixed inset-0 bg-bg-overlay flex items-center justify-center z-modal animate-[settings-fade-in_0.15s_ease-out]"
      onClick={onClose}
    >
      <div
        className="w-[400px] max-w-[90vw] bg-bg-sidebar border border-border rounded-lg shadow-modal py-7 px-8 flex flex-col items-center gap-2 relative animate-[settings-slide-in_0.2s_ease-out]"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
      >
        <button
          className="absolute top-2 right-2 w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-bright transition-colors cursor-pointer bg-transparent border-none"
          onClick={onClose}
          title={t(lang, 'aboutClose')}
        >
          <X size={16} />
        </button>

        <div className="flex items-center justify-center mb-1">
          <img src="/tauri.svg" alt="logo" width={72} height={72} />
        </div>

        <h2 className="text-xl font-semibold text-text-bright m-0">{appName}</h2>
        <p className="text-sm text-text-secondary m-0">
          {t(lang, 'aboutVersion')}: <code className="bg-bg-input px-1.5 py-0.5 rounded-sm text-text-primary font-mono">v{appVersion}</code>
        </p>
        <p className="text-sm text-text-primary text-center leading-relaxed my-2 mb-3">
          {t(lang, 'aboutDescription')}
        </p>

        <div className="w-full flex flex-col gap-1.5 py-2 border-t border-b border-border my-2 mb-4">
          <div className="flex justify-between text-sm">
            <span className="text-text-secondary">{t(lang, 'aboutAuthor')}</span>
            <span className="text-text-primary">{APP_AUTHOR}</span>
          </div>
          <div className="flex justify-between text-sm">
            <span className="text-text-secondary">{t(lang, 'aboutLicense')}</span>
            <span className="text-text-primary">{APP_LICENSE}</span>
          </div>
        </div>

        <div className="flex gap-2 mb-4">
          <button
            className="bg-transparent text-text-primary border border-border px-2.5 py-1 text-xs cursor-pointer rounded inline-flex items-center gap-1.5 transition-all hover:bg-bg-hover hover:border-accent hover:text-text-bright"
            onClick={() => void openUrl(APP_GITHUB)}
          >
            <Github size={14} />
            <span>{t(lang, 'aboutGithub')}</span>
            <ExternalLink size={10} className="opacity-60" />
          </button>
          <button
            className="bg-transparent text-text-primary border border-border px-2.5 py-1 text-xs cursor-pointer rounded inline-flex items-center gap-1.5 transition-all hover:bg-bg-hover hover:border-accent hover:text-text-bright"
            onClick={() => void openUrl(APP_DOCS)}
          >
            <span>{t(lang, 'aboutDocs')}</span>
            <ExternalLink size={10} className="opacity-60" />
          </button>
        </div>

        <div className="flex justify-center w-full">
          <button
            className="px-3 py-1.5 bg-bg-button text-text-inverse border-none rounded cursor-pointer text-sm text-center transition-colors hover:bg-bg-button-hover"
            onClick={onClose}
          >
            {t(lang, 'aboutClose')}
          </button>
        </div>
      </div>
    </div>
  );
}
