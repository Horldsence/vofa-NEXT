export interface StartupFlowState {
  settingsLoaded: boolean;
  showOnboarding: boolean;
  hasOpenedOnboarding: boolean;
  isOnboardingOpen: boolean;
  keychainPermissionPromptOpen: boolean;
  autoCheckUpdate: boolean;
}

export interface StartupFlowGate {
  onboardingSettled: boolean;
  showKeychainPermissionPrompt: boolean;
  canCheckForUpdates: boolean;
}

/** 启动弹窗严格串行:首次引导 → 钥匙串授权提醒 → 自动更新。 */
export function resolveStartupFlow(state: StartupFlowState): StartupFlowGate {
  const onboardingSettled =
    state.settingsLoaded &&
    (!state.showOnboarding ||
      (state.hasOpenedOnboarding && !state.isOnboardingOpen));
  const showKeychainPermissionPrompt =
    onboardingSettled && state.keychainPermissionPromptOpen;

  return {
    onboardingSettled,
    showKeychainPermissionPrompt,
    canCheckForUpdates:
      onboardingSettled &&
      !state.keychainPermissionPromptOpen &&
      state.autoCheckUpdate,
  };
}
