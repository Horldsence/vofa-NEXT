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
    <div className="sidebar">
      <div className="sidebar-header">
        <span>{t(lang, titleMap[view])}</span>
      </div>
      <div className="sidebar-content">
        {view === 'port' && <PortConfig />}
        {view === 'protocol' && <ProtocolConfigPanel />}
        {view === 'transport' && <TransportConfigPanel />}
        {view === 'widgets' && <WidgetPalette />}
        {view === 'ai' && (
          <div style={{ padding: 16, color: 'var(--text-secondary)', fontSize: 12, display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 8, marginTop: 32 }}>
            <Bot size={32} style={{ opacity: 0.4 }} />
            <p style={{ textAlign: 'center' }}>{t(lang, 'aiPlaceholder')}</p>
          </div>
        )}
      </div>
    </div>
  );
}