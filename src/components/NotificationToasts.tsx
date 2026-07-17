import { useEffect, useState } from 'react';
import { notify, type AppNotification } from '../lib/notifications';
import { useAppStore } from '../store/appStore';
import { t } from '../i18n';
import { XCircle, AlertTriangle, Info, X, ChevronDown } from 'lucide-react';

const MAX_VISIBLE = 5;

/// VSCode 风格通知 Toast 容器 — 右下角悬浮, 最多 5 条, 超出折叠
export function NotificationToasts() {
  const lang = useAppStore((s) => s.lang);
  const [list, setList] = useState<AppNotification[]>(notify.getList());

  useEffect(() => {
    return notify.subscribe(setList);
  }, []);

  const visible = list.slice(0, MAX_VISIBLE);
  const hiddenCount = list.length - MAX_VISIBLE;

  const dismiss = (id: string) => notify.dismiss(id);
  const dismissAll = () => notify.dismissAll();

  return (
    <div className="notif-container">
      {hiddenCount > 0 && (
        <button
          className="notif-overflow"
          onClick={dismissAll}
          title={t(lang, 'notifDismissAll')}
        >
          <ChevronDown size={12} />
          {t(lang, 'notifMore').replace('{{count}}', String(hiddenCount))}
        </button>
      )}
      {visible.map((n) => (
        <ToastItem
          key={n.id}
          notif={n}
          lang={lang}
          onDismiss={() => dismiss(n.id)}
        />
      ))}
    </div>
  );
}

interface ToastItemProps {
  notif: AppNotification;
  lang: 'zh' | 'en';
  onDismiss: () => void;
}

function ToastItem({ notif, lang, onDismiss }: ToastItemProps) {
  const [hovered, setHovered] = useState(false);
  const [expanded, setExpanded] = useState(false);

  const severityClass = `notif-${notif.severity}`;
  const Icon =
    notif.severity === 'error'
      ? XCircle
      : notif.severity === 'warn'
      ? AlertTriangle
      : Info;

  const severityLabel =
    notif.severity === 'error'
      ? t(lang, 'notifError')
      : notif.severity === 'warn'
      ? t(lang, 'notifWarn')
      : t(lang, 'notifInfo');

  const handleAction = (run: () => void) => {
    run();
    onDismiss();
  };

  return (
    <div
      className={`notif-toast ${severityClass} ${
        notif.count > 1 ? 'notif-collapsed' : ''
      }`}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      <div className="notif-icon">
        <Icon size={16} />
      </div>
      <div className="notif-body">
        <div className="notif-header">
          <span className="notif-title">
            {notif.title}
            {notif.count > 1 && (
              <span className="notif-count" title={t(lang, 'notifCollapsedHint')}>
                {' '}×{notif.count}
              </span>
            )}
          </span>
          <span className="notif-severity-tag">{severityLabel}</span>
        </div>
        <div
          className={`notif-message ${expanded ? 'expanded' : ''}`}
          onClick={() => setExpanded((v) => !v)}
        >
          {notif.message}
        </div>
        {notif.actions.length > 0 && (
          <div className="notif-actions">
            {notif.actions.map((a, i) => (
              <button
                key={i}
                className="notif-action-btn"
                onClick={() => handleAction(a.run)}
              >
                {a.label}
              </button>
            ))}
          </div>
        )}
      </div>
      <button
        className="notif-close"
        onClick={onDismiss}
        title={t(lang, 'notifDismiss')}
        aria-label={t(lang, 'notifDismiss')}
      >
        <X size={14} />
      </button>
      {/* 悬停时显示进度条 (info/warn 自动消失) */}
      {notif.severity !== 'error' && (
        <div
          className={`notif-progress ${hovered ? 'paused' : ''}`}
          key={notif.timestamp}
        />
      )}
    </div>
  );
}
