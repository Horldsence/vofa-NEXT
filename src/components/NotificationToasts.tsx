import { useEffect, useState } from 'react';
import { notify, type AppNotification } from '../lib/tauri/notifications';
import { useAppStore } from '../store/appStore';
import { t } from '../i18n';
import { XCircle, AlertTriangle, Info, X, ChevronDown } from 'lucide-react';

const MAX_VISIBLE = 5;

/// 统一提示 Toast 容器 — 右下角悬浮, 最多 5 条, 超出折叠
/// 卡片样式与首次使用引导提示框 (TourOverlay) 统一 (prompt-card 系列类)
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
    <div className="fixed right-4 bottom-8 z-200 flex flex-col items-end gap-2 pointer-events-none max-w-[380px]">
      {hiddenCount > 0 && (
        <button
          className="pointer-events-auto bg-bg-sidebar border border-border rounded-lg px-3 py-1.5 text-xs cursor-pointer inline-flex items-center gap-1 transition-colors hover:bg-bg-hover hover:text-text-bright"
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

  const iconColorClass =
    notif.severity === 'error' ? 'text-red' :
    notif.severity === 'warn' ? 'text-yellow' :
    'text-blue';

  const severityTagClass =
    notif.severity === 'error' ? 'text-red bg-red/10' :
    notif.severity === 'warn' ? 'text-yellow bg-yellow/10' :
    'text-blue bg-blue/10';

  const progressColorClass =
    notif.severity === 'warn' ? 'bg-yellow' :
    notif.severity === 'info' ? 'bg-blue' :
    'bg-text-secondary';

  const handleAction = (run: () => void) => {
    run();
    onDismiss();
  };

  return (
    <div
      className="prompt-card pointer-events-auto relative w-[360px] p-4 flex flex-col gap-2.5 overflow-hidden animate-[notif-slide-in_0.18s_ease-out]"
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      {/* 头部: 图标 + 标题 + 严重级别眉标 (引导风格小号大写标签) + 关闭 */}
      <div className="flex items-start gap-2">
        <div className={`flex-shrink-0 flex items-center justify-center pt-0.5 ${iconColorClass}`}>
          <Icon size={16} />
        </div>
        <div className="flex-1 min-w-0 flex items-center gap-2">
          <span className="prompt-card-title flex-1 min-w-0 overflow-hidden text-ellipsis whitespace-nowrap">
            {notif.title}
            {notif.count > 1 && (
              <span className="text-text-secondary font-normal font-mono text-xs cursor-help" title={t(lang, 'notifCollapsedHint')}>
                {' '}×{notif.count}
              </span>
            )}
          </span>
          <span className={`text-[10px] uppercase tracking-wider px-1.5 py-0.5 rounded-sm flex-shrink-0 ${severityTagClass}`}>
            {severityLabel}
          </span>
        </div>
        <button
          className="flex-shrink-0 bg-transparent border-none text-text-secondary cursor-pointer p-0.5 rounded-sm flex items-center justify-center transition-colors hover:bg-bg-hover hover:text-text-bright"
          onClick={onDismiss}
          title={t(lang, 'notifDismiss')}
          aria-label={t(lang, 'notifDismiss')}
        >
          <X size={14} />
        </button>
      </div>

      {/* 消息 — 与引导提示框正文同款排版 */}
      <div
        className={`prompt-card-body font-mono cursor-pointer overflow-hidden break-all whitespace-pre-wrap transition-[max-height] duration-200 ease-in-out ml-6 ${expanded ? 'max-h-[200px]' : 'max-h-[4.5em]'}`}
        onClick={() => setExpanded((v) => !v)}
      >
        {notif.message}
      </div>

      {notif.actions.length > 0 && (
        <div className="flex gap-1.5 mt-0.5 flex-wrap ml-6">
          {notif.actions.map((a, i) => (
            <button
              key={i}
              className="prompt-card-btn"
              onClick={() => handleAction(a.run)}
            >
              {a.label}
            </button>
          ))}
        </div>
      )}

      {/* 悬停时显示进度条 (info/warn 自动消失) */}
      {notif.severity !== 'error' && (
        <div
          className={`absolute left-0 bottom-0 h-0.5 opacity-40 notif-progress ${progressColorClass} ${hovered ? 'paused' : ''}`}
          key={notif.timestamp}
        />
      )}
    </div>
  );
}
