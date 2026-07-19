//! 可复用的内部提示条
//!
//! - 当 settings.general.showContextualTips 关闭时不渲染
//! - 支持当前会话级关闭 (dismissedTips)
//! - 可配置主/次操作按钮

import { X, Info } from 'lucide-react';
import { useAppStore } from '../../store/appStore';
import { useSettingsStore } from '../../store/settingsStore';
import { useOnboardingStore } from '../../store/onboardingStore';
import { t } from '../../i18n';

interface ContextualHintProps {
  id: string;
  message: string;
  action?: { label: string; onClick: () => void };
  secondaryAction?: { label: string; onClick: () => void };
}

export function ContextualHint({ id, message, action, secondaryAction }: ContextualHintProps) {
  const lang = useAppStore((s) => s.lang);
  const showTips = useSettingsStore((s) => s.settings.general.showContextualTips);
  const isDismissed = useOnboardingStore((s) => s.isTipDismissed(id));
  const dismiss = useOnboardingStore((s) => s.dismissTip);

  if (!showTips || isDismissed) return null;

  return (
    <div className="flex items-start gap-2 px-3 py-2 bg-accent/10 border-b border-accent/20 text-text-primary text-xs">
      <Info size={14} className="text-accent flex-shrink-0 mt-0.5" />
      <div className="flex-1 leading-relaxed min-w-0">{message}</div>
      <div className="flex items-center gap-2 flex-shrink-0">
        {secondaryAction && (
          <button
            className="text-accent hover:text-accent/80 transition-colors cursor-pointer"
            onClick={secondaryAction.onClick}
          >
            {secondaryAction.label}
          </button>
        )}
        {action && (
          <button
            className="px-2 py-0.5 bg-accent text-text-inverse rounded text-[11px] hover:bg-accent/80 transition-colors cursor-pointer"
            onClick={action.onClick}
          >
            {action.label}
          </button>
        )}
        <button
          className="w-5 h-5 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer"
          title={t(lang, 'dismissTip')}
          onClick={() => dismiss(id)}
        >
          <X size={12} />
        </button>
      </div>
    </div>
  );
}
