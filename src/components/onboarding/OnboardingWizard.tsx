//! 首次使用引导向导（高亮 + 操作提示）
//!
//! - 使用 TourOverlay 分步高亮界面元素
//! - 完成后自动关闭并设置 showOnboarding 为 false
//! - 可随时跳过

import { useAppStore } from '../../store/appStore';
import { useOnboardingStore } from '../../store/onboardingStore';
import { TourOverlay, type TourStep } from './TourOverlay';

export function OnboardingWizard() {
  const isOpen = useOnboardingStore((s) => s.isWizardOpen);
  const close = useOnboardingStore((s) => s.closeWizard);
  const complete = useOnboardingStore((s) => s.completeWizard);
  const setSidebarView = useAppStore((s) => s.setSidebarView);

  const steps: TourStep[] = [
    {
      titleKey: 'tourWelcomeTitle',
      contentKey: 'tourWelcomeContent',
    },
    {
      target: 'transport',
      titleKey: 'tourTransportTitle',
      contentKey: 'tourTransportContent',
      prepare: () => setSidebarView('transport'),
    },
    {
      target: 'protocol',
      titleKey: 'tourProtocolTitle',
      contentKey: 'tourProtocolContent',
      prepare: () => setSidebarView('protocol'),
    },
    {
      target: 'widgets',
      titleKey: 'tourWidgetsTitle',
      contentKey: 'tourWidgetsContent',
      prepare: () => setSidebarView('widgets'),
    },
    {
      target: 'connect',
      titleKey: 'tourConnectTitle',
      contentKey: 'tourConnectContent',
      prepare: () => setSidebarView('transport'),
    },
    {
      target: 'data-tabs',
      titleKey: 'tourDataTitle',
      contentKey: 'tourDataContent',
    },
    {
      target: 'help',
      titleKey: 'tourHelpTitle',
      contentKey: 'tourHelpContent',
    },
  ];

  return (
    <TourOverlay
      steps={steps}
      isOpen={isOpen}
      onComplete={complete}
      onSkip={close}
    />
  );
}
