import { describe, expect, it } from 'vitest';
import {
  resolveStartupFlow,
  shouldShowGuideAfterUpdate,
  type StartupFlowState,
} from '../startupFlow';

const READY: StartupFlowState = {
  settingsLoaded: true,
  showOnboarding: false,
  hasOpenedOnboarding: false,
  isOnboardingOpen: false,
  keychainPermissionPromptOpen: false,
  autoCheckUpdate: true,
};

describe('resolveStartupFlow', () => {
  it('keeps keychain and update behind the first-run onboarding', () => {
    expect(
      resolveStartupFlow({
        ...READY,
        showOnboarding: true,
        hasOpenedOnboarding: true,
        isOnboardingOpen: true,
        keychainPermissionPromptOpen: true,
      })
    ).toEqual({
      onboardingSettled: false,
      showKeychainPermissionPrompt: false,
      canCheckForUpdates: false,
    });
  });

  it('shows the keychain prompt after onboarding and keeps updates queued', () => {
    expect(
      resolveStartupFlow({
        ...READY,
        showOnboarding: true,
        hasOpenedOnboarding: true,
        keychainPermissionPromptOpen: true,
      })
    ).toEqual({
      onboardingSettled: true,
      showKeychainPermissionPrompt: true,
      canCheckForUpdates: false,
    });
  });

  it('allows auto update only after earlier startup dialogs settle', () => {
    expect(resolveStartupFlow(READY)).toEqual({
      onboardingSettled: true,
      showKeychainPermissionPrompt: false,
      canCheckForUpdates: true,
    });
  });
});

describe('shouldShowGuideAfterUpdate', () => {
  it('does not show the guide when no version was recorded (first run)', () => {
    expect(shouldShowGuideAfterUpdate(null, '1.2.0')).toBe(false);
  });

  it('does not show the guide when the version is unchanged', () => {
    expect(shouldShowGuideAfterUpdate('1.2.0', '1.2.0')).toBe(false);
  });

  it('shows the guide once after the version changes', () => {
    expect(shouldShowGuideAfterUpdate('1.1.0', '1.2.0')).toBe(true);
  });
});
