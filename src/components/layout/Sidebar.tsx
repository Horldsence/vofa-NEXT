import { useAppStore } from '../../store/appStore';
import type { SidebarView } from '../../store/appStore';
import { t } from '../../i18n';
import { PortConfig } from '../panels/PortConfig';
import { ProtocolConfigPanel } from '../panels/ProtocolConfigPanel';
import { TransportConfigPanel } from '../panels/TransportConfigPanel';
import { WidgetPalette } from '../panels/WidgetPalette';
import { Bot } from 'lucide-react';

interface SidebarProps {
  view: SidebarView;
}

/// 侧边栏容器 — 根据当前视图切换面板
export function Sidebar({ view }: SidebarProps) {
  const lang = useAppStore((s) => s.lang);

  const titleMap: Record<SidebarView, Parameters<typeof t>[1]> = {
    port: 'portConfig',
    protocol: 'protocolEngine',
    transport: 'transportType',
    widgets: 'widgetPalette',
    ai: 'aiAssistant',
  };

  return (
    <div className="bg-bg-sidebar flex flex-col min-w-[200px] overflow-hidden">
      <div className="px-4 py-2 text-xs font-semibold uppercase tracking-wider text-text-secondary flex items-center justify-between flex-shrink-0">
        <span>{t(lang, titleMap[view])}</span>
      </div>
      <div className="flex-1 overflow-y-auto px-3 pb-3">
        {view === 'port' && <PortConfig />}
        {view === 'protocol' && <ProtocolConfigPanel />}
        {view === 'transport' && <TransportConfigPanel />}
        {view === 'widgets' && <WidgetPalette />}
        {view === 'ai' && (
          <div className="p-4 text-text-secondary text-xs flex flex-col items-center gap-2 mt-8">
            <Bot size={32} className="opacity-40" />
            <p className="text-center">{t(lang, 'aiPlaceholder')}</p>
          </div>
        )}
      </div>
    </div>
  );
}