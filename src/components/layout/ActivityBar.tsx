import {
  Usb,
  Binary,
  Network,
  LayoutGrid,
  Bot,
  Settings,
  Info,
} from 'lucide-react';
import type { SidebarView } from '../../store/appStore';
import { useAppStore } from '../../store/appStore';
import { useSettingsStore } from '../../store/settingsStore';
import { t } from '../../i18n';

interface ActivityBarProps {
  activeView: SidebarView | null;
  onSelect: (view: SidebarView) => void;
}

/// 左侧活动栏 — VSCode 风格图标导航
/// 顺序符合配置操作流: 数据接口 → 协议引擎 → 串口配置 → 控件
/// AI 入口已隐藏 (保留代码, 通过修改 HIDE_AI_VIEWS 控制可见性)
const HIDE_AI_VIEWS = true;

export function ActivityBar({ activeView, onSelect }: ActivityBarProps) {
  const lang = useAppStore((s) => s.lang);
  const openSettings = useSettingsStore((s) => s.open);
  const openAbout = useSettingsStore((s) => s.openAbout);

  const items: { view: SidebarView; icon: React.ReactNode; key: Parameters<typeof t>[1] }[] = [
    { view: 'transport', icon: <Network size={22} />, key: 'transportType' },
    { view: 'protocol', icon: <Binary size={22} />, key: 'protocolEngine' },
    { view: 'port', icon: <Usb size={22} />, key: 'portConfig' },
    { view: 'widgets', icon: <LayoutGrid size={22} />, key: 'widgetPalette' },
    { view: 'ai', icon: <Bot size={22} />, key: 'aiAssistant' },
  ];

  const visibleItems = HIDE_AI_VIEWS
    ? items.filter((i) => i.view !== 'ai')
    : items;

  return (
    <div className="activity-bar">
      {visibleItems.map((item) => (
        <div
          key={item.view}
          className={`activity-icon ${activeView === item.view ? 'active' : ''}`}
          title={t(lang, item.key)}
          onClick={() => onSelect(item.view)}
        >
          {item.icon}
        </div>
      ))}
      <div style={{ flex: 1 }} />
      <div
        className="activity-icon"
        title={t(lang, 'about')}
        onClick={openAbout}
      >
        <Info size={22} />
      </div>
      <div
        className="activity-icon"
        title={t(lang, 'settings')}
        onClick={() => openSettings()}
      >
        <Settings size={22} />
      </div>
    </div>
  );
}
