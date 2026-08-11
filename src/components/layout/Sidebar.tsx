import { memo } from 'react';
import { useAppStore } from '../../store/appStore';
import type { SidebarView } from '../../store/appStore';
import { useContextMenu } from '../../lib/hooks/useContextMenu';
import { transitionStore } from '../../lib/utils/transitionStore';
import { t } from '../../i18n';
import { TransportConfigPanel } from '../panels/TransportConfigPanel';
import { ProtocolSection } from '../panels/ProtocolSection';
import { WidgetPalette } from '../panels/WidgetPalette';
import { PanelLeft, RefreshCw } from 'lucide-react';
import { AnimatedSwitch } from '../ui/AnimatedSwitch';
import { useLayoutStore } from '../../store/layoutStore';

interface SidebarProps {
  view: SidebarView;
}

/// 侧边栏容器 — 根据当前视图切换面板
export const Sidebar = memo(function Sidebar({ view }: SidebarProps) {
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
      onClick: () => transitionStore(() => toggleSidebar(sidebarView)),
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

  // 标题栏为拖拽源 — 拖到窗口左/右边缘可切换停靠侧
  const setDraggingSidebar = useLayoutStore((s) => s.setDraggingSidebar);

  return (
    <div className="bg-bg-sidebar flex flex-col h-full w-full min-w-[200px] overflow-hidden" onContextMenu={onContextMenu}>
      <div
        className="px-4 h-9 text-xs font-semibold uppercase tracking-wider text-text-secondary flex items-center justify-between flex-shrink-0 cursor-grab active:cursor-grabbing"
        draggable
        onDragStart={(e) => {
          e.dataTransfer.setData('text/plain', 'panel:sidebar');
          e.dataTransfer.effectAllowed = 'move';
          setDraggingSidebar(true);
        }}
        onDragEnd={() => setDraggingSidebar(false)}
        title={t(lang, 'dragToDock')}
      >
        <span>{t(lang, titleMap[view])}</span>
      </div>
      <div className="flex-1 overflow-y-auto px-3 pb-3">
        <AnimatedSwitch switchKey={view} order={['transport', 'protocol', 'widgets']} axis="y">
          {view === 'transport' && <TransportConfigPanel />}
          {view === 'protocol' && <ProtocolSection />}
          {view === 'widgets' && <WidgetPalette />}
        </AnimatedSwitch>
      </div>
    </div>
  );
});
