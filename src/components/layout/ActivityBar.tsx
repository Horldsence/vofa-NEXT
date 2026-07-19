import {
  Cable,
  Binary,
  LayoutGrid,
  Settings,
  Info,
  HelpCircle,
  PanelLeft,
} from 'lucide-react';
import clsx from 'clsx';
import type { SidebarView } from '../../store/appStore';
import { useAppStore } from '../../store/appStore';
import { useSettingsStore } from '../../store/settingsStore';
import { useOnboardingStore } from '../../store/onboardingStore';
import { useContextMenu } from '../../lib/useContextMenu';
import { t } from '../../i18n';

interface ActivityBarProps {
  activeView: SidebarView | null;
  onSelect: (view: SidebarView) => void;
}

/// 左侧活动栏 — VSCode 风格图标导航
/// 顺序符合配置操作流: 数据接口 → 协议引擎 → 控件
export function ActivityBar({ activeView, onSelect }: ActivityBarProps) {
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
      onClick: () => toggleSidebar(sidebarView),
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
    { view: 'transport', icon: <Cable size={22} />, key: 'dataInterface' },
    { view: 'protocol', icon: <Binary size={22} />, key: 'protocolEngine' },
    { view: 'widgets', icon: <LayoutGrid size={22} />, key: 'widgetPalette' },
  ];

  return (
    <div className="w-12 bg-bg-activity flex flex-col items-center pt-1 flex-shrink-0" onContextMenu={onContextMenu}>
      {items.map((item) => (
        <div
          key={item.view}
          data-tour={item.view}
          className={clsx(
            "w-12 h-12 flex items-center justify-center cursor-pointer text-text-inverse/60 relative transition-colors duration-150 hover:text-text-inverse hover:bg-text-inverse/5",
            activeView === item.view && "text-text-inverse before:content-[''] before:absolute before:left-0 before:top-0 before:bottom-0 before:w-0.5 before:bg-text-inverse"
          )}
          title={t(lang, item.key)}
          onClick={() => onSelect(item.view)}
        >
          {item.icon}
        </div>
      ))}
      <div className="flex-1" />
      <div
        data-tour="help"
        className="w-12 h-12 flex items-center justify-center cursor-pointer text-text-inverse/60 hover:text-text-inverse hover:bg-text-inverse/5 transition-colors duration-150"
        title={t(lang, 'helpCenterOpen')}
        onClick={openHelp}
      >
        <HelpCircle size={22} />
      </div>
      <div
        className="w-12 h-12 flex items-center justify-center cursor-pointer text-text-inverse/60 hover:text-text-inverse hover:bg-text-inverse/5 transition-colors duration-150"
        title={t(lang, 'about')}
        onClick={openAbout}
      >
        <Info size={22} />
      </div>
      <div
        className="w-12 h-12 flex items-center justify-center cursor-pointer text-text-inverse/60 hover:text-text-inverse hover:bg-text-inverse/5 transition-colors duration-150"
        title={t(lang, 'settings')}
        onClick={() => openSettings()}
      >
        <Settings size={22} />
      </div>
    </div>
  );
}
