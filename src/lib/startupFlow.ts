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

/** 版本更新后是否应弹出操作指南: 仅当记录过旧版本且与当前版本不同
 *  (lastSeenVersion 为 null 表示首次安装或老用户首次运行含本功能的版本,
 *   前者由 showOnboarding 首次引导覆盖, 后者避免本功能上线当次打扰) */
export function shouldShowGuideAfterUpdate(
  lastSeenVersion: string | null,
  currentVersion: string
): boolean {
  return lastSeenVersion !== null && lastSeenVersion !== currentVersion;
}
