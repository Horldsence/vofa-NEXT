import { memo } from 'react';
import {
  Cable,
  LayoutGrid,
  Layers,
  Settings,
  Info,
  HelpCircle,
  PanelLeft,
  Rocket,
} from 'lucide-react';
import clsx from 'clsx';
import type { SidebarView } from '../../store/appStore';
import { useAppStore } from '../../store/appStore';
import { useSettingsStore } from '../../store/settingsStore';
import { useOnboardingStore } from '../../store/onboardingStore';
import { useContextMenu } from '../../lib/hooks/useContextMenu';
import { transitionStore } from '../../lib/utils/transitionStore';
import { t } from '../../i18n';

interface ActivityBarProps {
  activeView: SidebarView | null;
  onSelect: (view: SidebarView) => void;
}

/// 左侧活动栏 — VSCode 风格图标导航
/// 顺序符合配置操作流: 数据接口 → 协议引擎 → 控件
export const ActivityBar = memo(function ActivityBar({ activeView, onSelect }: ActivityBarProps) {
  const lang = useAppStore((s) => s.lang);
  const sidebarView = useAppStore((s) => s.sidebarView);
  const sidebarVisible = useAppStore((s) => s.sidebarVisible);
  const toggleSidebar = useAppStore((s) => s.toggleSidebar);
  const refreshPorts = useAppStore((s) => s.refreshPorts);
  const openSettings = useSettingsStore((s) => s.open);
  const openAbout = useSettingsStore((s) => s.openAbout);
  const openHelp = useOnboardingStore((s) => s.openHelp);

  const onContextMenu = useContextMenu([
    {
      id: 'toggle-sidebar',
      label: sidebarVisible ? t(lang, 'contextMenuHideSidebar') : t(lang, 'contextMenuShowSidebar'),
      icon: <PanelLeft />,
      onClick: () => transitionStore(() => toggleSidebar(sidebarView)),
    },
    { kind: 'separator' },
    {
      id: 'refresh-ports',
      label: t(lang, 'refresh'),
      icon: <Cable />,
      onClick: () => refreshPorts(),
    },
    {
      id: 'settings',
      label: t(lang, 'settings'),
      icon: <Settings />,
      onClick: openSettings,
    },
    {
      id: 'about',
      label: t(lang, 'about'),
      icon: <Info />,
      onClick: openAbout,
    },
    {
      id: 'help',
      label: t(lang, 'helpCenterOpen'),
      icon: <HelpCircle />,
      onClick: openHelp,
    },
  ]);

  const items: { view: SidebarView; icon: React.ReactNode; key: Parameters<typeof t>[1] }[] = [
    { view: 'quickstart', icon: <Rocket size={22} />, key: 'quickStart' },
    { view: 'widgets', icon: <LayoutGrid size={22} />, key: 'widgetPalette' },
    { view: 'panels', icon: <Layers size={22} />, key: 'menuPanel' },
  ];

  return (
    // 根节点用 w-full 而非 w-12: 外层 module-card 有 1px 边框, 内容盒仅 46px;
    // 若这里固定 48px 会向两侧溢出 1px, 导致激活高亮块相对外围框左右间隙不等距 (5px / 3px)。
    // w-full 使按钮在可见区域内居中, 两侧均为 4px。
    <div className="w-full h-full bg-bg-activity flex flex-col items-center py-1 gap-1 flex-shrink-0" onContextMenu={onContextMenu}>
      {items.map((item) => (
        <div
          key={item.view}
          data-tour={item.view}
          className={clsx(
            "w-10 h-10 mx-1 rounded-md flex items-center justify-center cursor-pointer transition-colors duration-150 active:bg-accent-active",
            activeView === item.view
              ? "text-text-bright bg-bg-active"
              : "text-text-secondary hover:text-text-primary hover:bg-bg-hover"
          )}
          title={t(lang, item.key)}
          onClick={() => transitionStore(() => onSelect(item.view))}
        >
          {item.icon}
        </div>
      ))}
      <div className="flex-1" />
      <div
        data-tour="help"
        className="w-10 h-10 mx-1 rounded-md flex items-center justify-center cursor-pointer text-text-secondary hover:text-text-primary hover:bg-bg-hover active:bg-accent-active transition-colors duration-150"
        title={t(lang, 'helpCenterOpen')}
        onClick={openHelp}
      >
        <HelpCircle size={22} />
      </div>
      <div
        className="w-10 h-10 mx-1 rounded-md flex items-center justify-center cursor-pointer text-text-secondary hover:text-text-primary hover:bg-bg-hover active:bg-accent-active transition-colors duration-150"
        title={t(lang, 'about')}
        onClick={openAbout}
      >
        <Info size={22} />
      </div>
      <div
        className="w-10 h-10 mx-1 rounded-md flex items-center justify-center cursor-pointer text-text-secondary hover:text-text-primary hover:bg-bg-hover active:bg-accent-active transition-colors duration-150"
        title={t(lang, 'settings')}
        onClick={() => openSettings()}
      >
        <Settings size={22} />
      </div>
    </div>
  );
});
