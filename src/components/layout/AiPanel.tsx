import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { Bot, Send } from 'lucide-react';

interface AiPanelProps {
  /// 在侧边栏内嵌时使用紧凑模式
  inSidebar?: boolean;
}

/// AI 面板 — 预留占位
export function AiPanel({ inSidebar = false }: AiPanelProps) {
  const lang = useAppStore((s) => s.lang);

  if (inSidebar) {
    return (
      <div style={{ padding: 12, color: 'var(--text-secondary)', fontSize: 12 }}>
        <Bot size={32} style={{ opacity: 0.4, marginBottom: 8 }} />
        <p>{t(lang, 'aiPlaceholder')}</p>
      </div>
    );
  }

  return (
    <div className="panel ai-panel">
      <div className="panel-header">
        <span>{t(lang, 'aiAssistant')}</span>
        <Bot size={14} style={{ opacity: 0.6 }} />
      </div>
      <div className="ai-content">
        <Bot size={48} style={{ opacity: 0.3 }} />
        <p style={{ textAlign: 'center' }}>{t(lang, 'aiPlaceholder')}</p>
      </div>
      <div className="ai-input-bar">
        <input
          type="text"
          className="send-input"
          placeholder={t(lang, 'inputMessage')}
          disabled
        />
        <button className="btn-icon" disabled>
          <Send size={14} />
        </button>
      </div>
    </div>
  );
}
