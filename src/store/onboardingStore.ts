//! 使用引导状态管理
//!
//! - 首次引导由 settings.general.showOnboarding 控制持久化开关
//! - 帮助中心可手动随时打开
//! - 内部提示支持当前会话级关闭 (dismissedTips)

import { create } from 'zustand';
import { useSettingsStore } from './settingsStore';

interface OnboardingStore {
  isWizardOpen: boolean;
  isHelpOpen: boolean;
  dismissedTips: Set<string>;
  hasOpenedThisSession: boolean;

  openWizard: () => void;
  closeWizard: () => void;
  completeWizard: () => void;

  openHelp: () => void;
  closeHelp: () => void;

  dismissTip: (id: string) => void;
  isTipDismissed: (id: string) => boolean;
}

export const useOnboardingStore = create<OnboardingStore>((set, get) => ({
  isWizardOpen: false,
  isHelpOpen: false,
  dismissedTips: new Set(),
  hasOpenedThisSession: false,

  openWizard: () => set({ isWizardOpen: true, hasOpenedThisSession: true }),
  closeWizard: () => set({ isWizardOpen: false }),
  completeWizard: () => {
    // 关闭引导并持久化“不再自动显示”
    set({ isWizardOpen: false });
    useSettingsStore.getState().update('general', 'showOnboarding', false);
  },

  openHelp: () => set({ isHelpOpen: true }),
  closeHelp: () => set({ isHelpOpen: false }),

  dismissTip: (id) =>
    set((s) => {
      const next = new Set(s.dismissedTips);
      next.add(id);
      return { dismissedTips: next };
    }),
  isTipDismissed: (id) => get().dismissedTips.has(id),
}));
