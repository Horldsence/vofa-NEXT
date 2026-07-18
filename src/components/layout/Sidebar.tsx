import { useAppStore } from '../../store/appStore';
import type { SidebarView } from '../../store/appStore';
import { t } from '../../i18n';
import { TransportConfigPanel } from '../panels/TransportConfigPanel';
import { ProtocolSection } from '../panels/ProtocolSection';
import { WidgetPalette } from '../panels/WidgetPalette';

interface SidebarProps {
  view: SidebarView;
}

/// 侧边栏容器 — 根据当前视图切换面板
export function Sidebar({ view }: SidebarProps) {
  const lang = useAppStore((s) => s.lang);

  const titleMap: Record<SidebarView, Parameters<typeof t>[1]> = {
    transport: 'dataInterface',
    protocol: 'protocolEngine',
    widgets: 'widgetPalette',
  };

  return (
    <div className="bg-bg-sidebar flex flex-col min-w-[200px] overflow-hidden">
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
