import {
  Cable,
  Binary,
  LayoutGrid,
  Settings,
  Info,
} from 'lucide-react';
import clsx from 'clsx';
import type { SidebarView } from '../../store/appStore';
import { useAppStore } from '../../store/appStore';
import { useSettingsStore } from '../../store/settingsStore';
import { t } from '../../i18n';

interface ActivityBarProps {
  activeView: SidebarView | null;
  onSelect: (view: SidebarView) => void;
}

/// 左侧活动栏 — VSCode 风格图标导航
/// 顺序符合配置操作流: 数据接口 → 协议引擎 → 控件
export function ActivityBar({ activeView, onSelect }: ActivityBarProps) {
  const lang = useAppStore((s) => s.lang);
  const openSettings = useSettingsStore((s) => s.open);
  const openAbout = useSettingsStore((s) => s.openAbout);

  const items: { view: SidebarView; icon: React.ReactNode; key: Parameters<typeof t>[1] }[] = [
    { view: 'transport', icon: <Cable size={22} />, key: 'dataInterface' },
    { view: 'protocol', icon: <Binary size={22} />, key: 'protocolEngine' },
    { view: 'widgets', icon: <LayoutGrid size={22} />, key: 'widgetPalette' },
  ];

  return (
    <div className="w-12 bg-bg-activity flex flex-col items-center pt-1 flex-shrink-0">
      {items.map((item) => (
        <div
          key={item.view}
          className={clsx(
            "w-12 h-12 flex items-center justify-center cursor-pointer text-text-secondary relative transition-colors duration-150 hover:text-text-primary",
            activeView === item.view && "text-text-bright before:content-[''] before:absolute before:left-0 before:top-0 before:bottom-0 before:w-0.5 before:bg-text-bright"
          )}
          title={t(lang, item.key)}
          onClick={() => onSelect(item.view)}
        >
          {item.icon}
        </div>
      ))}
      <div className="flex-1" />
      <div
        className="w-12 h-12 flex items-center justify-center cursor-pointer text-text-secondary hover:text-text-primary transition-colors duration-150"
        title={t(lang, 'about')}
        onClick={openAbout}
      >
        <Info size={22} />
      </div>
      <div
        className="w-12 h-12 flex items-center justify-center cursor-pointer text-text-secondary hover:text-text-primary transition-colors duration-150"
        title={t(lang, 'settings')}
        onClick={() => openSettings()}
      >
        <Settings size={22} />
      </div>
    </div>
  );
}
