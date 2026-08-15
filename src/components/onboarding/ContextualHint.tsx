//! 可复用的内部提示条
//!
//! - 当 settings.general.showContextualTips 关闭时不渲染
//! - 支持当前会话级关闭 (dismissedTips)
//! - 可配置主/次操作按钮
//! - 样式与首次使用引导提示框统一 (prompt-card 系列类)

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
    <div className="prompt-card prompt-card--bar flex items-start gap-2 px-3 py-2 text-text-primary text-xs">
      <Info size={14} className="text-accent flex-shrink-0 mt-0.5" />
      <div className="prompt-card-body flex-1 min-w-0">{message}</div>
      <div className="flex items-center gap-2 flex-shrink-0">
        {secondaryAction && (
          <button
            className="prompt-card-btn-ghost"
            onClick={secondaryAction.onClick}
          >
            {secondaryAction.label}
          </button>
        )}
        {action && (
          <button
            className="prompt-card-btn"
            onClick={action.onClick}
          >
            {action.label}
          </button>
        )}
        <button
          className="prompt-card-btn-ghost"
          title={t(lang, 'dismissTip')}
          onClick={() => dismiss(id)}
        >
          <X size={12} />
        </button>
      </div>
    </div>
  );
}
