//! 帮助中心弹窗
//!
//! - 章节式布局，覆盖快速入门、数据接口、协议、控件、CAN、逻辑分析仪、自定义控件
//! - 从 ActivityBar 帮助图标或设置中打开

import { X, HelpCircle } from 'lucide-react';
import { useAppStore } from '../../store/appStore';
import { useOnboardingStore } from '../../store/onboardingStore';
import { t } from '../../i18n';
import { HELP_SECTIONS } from './helpContent';

export function HelpCenterModal() {
  const lang = useAppStore((s) => s.lang);
  const isOpen = useOnboardingStore((s) => s.isHelpOpen);
  const close = useOnboardingStore((s) => s.closeHelp);

  if (!isOpen) return null;

  return (
    <div
      className="fixed inset-0 bg-black/50 flex items-center justify-center z-modal animate-[settings-fade-in_0.15s_ease-out]"
      onClick={close}
    >
      <div
        className="flex flex-col bg-bg-sidebar border border-border rounded-lg overflow-hidden shadow-[0_20px_60px_rgba(0,0,0,0.6)] animate-[settings-slide-in_0.2s_ease-out]"
        onClick={(e) => e.stopPropagation()}
        style={{ width: '80vw', height: '85vh', maxWidth: 1000, maxHeight: 800 }}
      >
        <div className="flex items-center gap-2 px-3 py-2 bg-bg-panel-header border-b border-border text-text-primary font-semibold">
          <HelpCircle size={16} />
          <span>{t(lang, 'helpCenterTitle')}</span>
          <button
            className="w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer ml-auto"
            onClick={close}
          >
            <X size={14} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto px-5 py-4 flex flex-col gap-5">
          {HELP_SECTIONS.map((section) => {
            const Icon = section.icon;
            const steps = (t(lang, section.stepsKey) || '').split('\n').filter(Boolean);
            return (
              <section key={section.id} className="flex flex-col gap-2">
                <h2 className="flex items-center gap-1.5 text-base font-semibold text-text-primary m-0 pb-1 border-b border-border">
                  <Icon size={14} className="text-accent flex-shrink-0" />
                  {t(lang, section.titleKey)}
                </h2>
                <p className="m-0 text-sm text-text-secondary leading-[1.5]">
                  {t(lang, section.descKey)}
                </p>
                {steps.length > 0 && (
                  <ol className="m-0 pl-5 text-sm text-text-primary leading-[1.7] list-decimal">
                    {steps.map((step, idx) => (
                      <li key={idx}>{step}</li>
                    ))}
                  </ol>
                )}
              </section>
            );
          })}
        </div>

        <div className="px-3 py-2 flex justify-end bg-bg-panel-header border-t border-border">
          <button
            className="px-3 py-1.5 bg-bg-button text-text-inverse border-none rounded cursor-pointer text-sm text-center transition-colors hover:bg-bg-button-hover"
            onClick={close}
          >
            {t(lang, 'helpClose')}
          </button>
        </div>
      </div>
    </div>
  );
}
