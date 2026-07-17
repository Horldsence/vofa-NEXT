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
    <div className="settings-overlay" onClick={onClose}>
      <div
        className="about-modal"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
      >
        <button className="settings-close-btn" onClick={onClose} title={t(lang, 'aboutClose')}>
          <X size={16} />
        </button>

        <div className="about-icon">
          <img src="/tauri.svg" alt="logo" width={72} height={72} />
        </div>

        <h2 className="about-name">{appName}</h2>
        <p className="about-version">
          {t(lang, 'aboutVersion')}: <code>v{appVersion}</code>
        </p>
        <p className="about-description">{t(lang, 'aboutDescription')}</p>

        <div className="about-info-grid">
          <div className="about-info-row">
            <span className="about-info-label">{t(lang, 'aboutAuthor')}</span>
            <span className="about-info-value">{APP_AUTHOR}</span>
          </div>
          <div className="about-info-row">
            <span className="about-info-label">{t(lang, 'aboutLicense')}</span>
            <span className="about-info-value">{APP_LICENSE}</span>
          </div>
        </div>

        <div className="about-links">
          <button
            className="btn-secondary about-link-btn"
            onClick={() => void openUrl(APP_GITHUB)}
          >
            <Github size={14} />
            <span>{t(lang, 'aboutGithub')}</span>
            <ExternalLink size={10} style={{ opacity: 0.6 }} />
          </button>
          <button
            className="btn-secondary about-link-btn"
            onClick={() => void openUrl(APP_DOCS)}
          >
            <span>{t(lang, 'aboutDocs')}</span>
            <ExternalLink size={10} style={{ opacity: 0.6 }} />
          </button>
        </div>

        <div className="about-footer">
          <button className="btn-primary" onClick={onClose}>
            {t(lang, 'aboutClose')}
          </button>
        </div>
      </div>
    </div>
  );
}
