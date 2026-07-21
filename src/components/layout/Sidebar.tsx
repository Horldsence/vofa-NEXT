import { useAppStore } from '../../store/appStore';
import type { SidebarView } from '../../store/appStore';
import { useContextMenu } from '../../lib/useContextMenu';
import { t } from '../../i18n';
import { TransportConfigPanel } from '../panels/TransportConfigPanel';
import { ProtocolSection } from '../panels/ProtocolSection';
import { WidgetPalette } from '../panels/WidgetPalette';
import { PanelLeft, RefreshCw } from 'lucide-react';

interface SidebarProps {
  view: SidebarView;
}

/// 侧边栏容器 — 根据当前视图切换面板
export function Sidebar({ view }: SidebarProps) {
  const lang = useAppStore((s) => s.lang);
  const sidebarView = useAppStore((s) => s.sidebarView);
  const sidebarVisible = useAppStore((s) => s.sidebarVisible);
  const toggleSidebar = useAppStore((s) => s.toggleSidebar);
  const refreshPorts = useAppStore((s) => s.refreshPorts);

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
      icon: <RefreshCw />,
      onClick: () => refreshPorts(),
    },
  ]);

  const titleMap: Record<SidebarView, Parameters<typeof t>[1]> = {
    transport: 'dataInterface',
    protocol: 'protocolEngine',
    widgets: 'widgetPalette',
  };

  return (
    <div className="bg-bg-sidebar flex flex-col h-full w-full min-w-[200px] overflow-hidden" onContextMenu={onContextMenu}>
      <div className="px-4 py-2 text-xs font-semibold uppercase tracking-wider text-text-secondary flex items-center justify-between flex-shrink-0">
        <span>{t(lang, titleMap[view])}</span>
      </div>
      <div className="flex-1 overflow-y-auto px-3 pb-3">
        {view === 'transport' && <TransportConfigPanel />}
        {view === 'protocol' && <ProtocolSection />}
        {view === 'widgets' && <WidgetPalette />}
      </div>
    </div>
  );
}
