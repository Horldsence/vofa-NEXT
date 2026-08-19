import { memo } from 'react';
import { useAppStore } from '../../store/appStore';
import type { SidebarView } from '../../store/appStore';
import { useContextMenu } from '../../lib/hooks/useContextMenu';
import { transitionStore } from '../../lib/utils/transitionStore';
import { dockDrag } from '../../lib/dockDrag';
import { t } from '../../i18n';
import { WidgetPalette } from '../panels/widgetPalette';
import { QuickStartPanel } from '../panels/QuickStartPanel';
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
    quickstart: 'quickStart',
    widgets: 'widgetPalette',
  };

  // 标题栏为拖拽源 — 拖到窗口左/右边缘可切换停靠侧 (dockDrag 控制器)
  const draggingSidebar = useLayoutStore((s) => s.draggingSidebar);

  return (
    <div
      className={`bg-bg-sidebar flex flex-col h-full w-full min-w-[200px] overflow-hidden ${
        draggingSidebar ? 'ring-2 ring-inset ring-accent' : ''
      }`}
      onContextMenu={onContextMenu}
    >
      <div
        className="px-4 h-9 text-xs font-semibold uppercase tracking-wider text-text-secondary flex items-center justify-between flex-shrink-0 cursor-grab active:cursor-grabbing border-b border-border-subtle select-none"
        onPointerDown={(e) => {
          if (e.button !== 0) return;
          if ((e.target as HTMLElement).closest('button, input')) return;
          dockDrag.begin(e, { kind: 'sidebar', label: t(lang, titleMap[view]) });
        }}
        title={t(lang, 'dragToDock')}
      >
        <span>{t(lang, titleMap[view])}</span>
      </div>
      {/* 内容区 — 不自己滚动, 约束高度让各面板 (QuickStart/WidgetPalette) 内部滚动,
          保证面板顶部工具条 (如控件面板跳转条) 始终固定在可视区上部 */}
      <div className="flex-1 min-h-0 overflow-hidden px-3 py-3">
        <AnimatedSwitch switchKey={view} order={['quickstart', 'widgets']} axis="y" className="h-full">
          {view === 'quickstart' && <QuickStartPanel />}
          {view === 'widgets' && <WidgetPalette />}
        </AnimatedSwitch>
      </div>
    </div>
  );
});
