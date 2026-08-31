//! 更新状态栏指示器
//!
//! - checking: 旋转图标
//! - available: 醒目圆点 + 新版本号, 点击打开更新弹窗
//! - downloading: 进度百分比
//! - ready: "重启更新", 点击 relaunch
//! - up-to-date / error: 仅手动检查后显示 (auto 失败/无更新保持静默, 不打扰用户)
//! - idle: 不渲染

import { useEffect, useState } from 'react';
import { RefreshCw, AlertTriangle } from 'lucide-react';
import { useAppStore } from '../../store/appStore';
import { useUpdateStore } from '../../store/updateStore';
import { t } from '../../i18n';
import { activateOnKeyboard } from '../../lib/utils/a11y';

/// "已是最新" 提示的显示时长 (ms)
const UP_TO_DATE_HINT_MS = 5_000;

export function UpdateIndicator() {
  const lang = useAppStore((s) => s.lang);
  const status = useUpdateStore((s) => s.status);
  const lastTrigger = useUpdateStore((s) => s.lastTrigger);
  const updateInfo = useUpdateStore((s) => s.updateInfo);
  const progress = useUpdateStore((s) => s.progress);
  const error = useUpdateStore((s) => s.error);
  const openDialog = useUpdateStore((s) => s.openDialog);
  const relaunch = useUpdateStore((s) => s.relaunch);

  /// 手动检查后的 "已是最新" 短暂提示
  const [showUpToDate, setShowUpToDate] = useState(false);
  useEffect(() => {
    if (status !== 'up-to-date' || lastTrigger !== 'manual') return;
    setShowUpToDate(true);
    const timer = setTimeout(() => setShowUpToDate(false), UP_TO_DATE_HINT_MS);
    return () => clearTimeout(timer);
  }, [status, lastTrigger]);

  if (status === 'checking') {
    return (
      <div className="flex items-center gap-1.5 px-1.5" title={t(lang, 'updateChecking')}>
        <RefreshCw size={12} className="animate-spin text-text-secondary" />
      </div>
    );
  }

  if (status === 'available' && updateInfo) {
    return (
      <div
        className="flex items-center gap-1.5 px-1.5 cursor-pointer hover:bg-bg-hover rounded"
        title={t(lang, 'updateAvailableTitle')}
        onClick={openDialog}
        onKeyDown={activateOnKeyboard}
        role="button"
        tabIndex={0}
      >
        <span className="w-2 h-2 rounded-full bg-accent animate-pulse inline-block" />
        <span className="text-accent font-mono text-[10px]">v{updateInfo.version}</span>
      </div>
    );
  }

  if (status === 'downloading') {
    return (
      <div
        className="flex items-center gap-1.5 px-1.5 cursor-pointer hover:bg-bg-hover rounded tabular-nums"
        title={t(lang, 'updateDownloading')}
        onClick={openDialog}
        onKeyDown={activateOnKeyboard}
        role="button"
        tabIndex={0}
      >
        <span className="text-text-secondary font-mono text-[10px]">{progress}%</span>
      </div>
    );
  }

  if (status === 'ready') {
    return (
      <div
        className="flex items-center gap-1.5 px-1.5 cursor-pointer hover:bg-bg-hover rounded"
        title={t(lang, 'updateReadyHint')}
        onClick={() => void relaunch()}
        onKeyDown={activateOnKeyboard}
        role="button"
        tabIndex={0}
      >
        <span className="w-2 h-2 rounded-full bg-green inline-block" />
        <span className="text-green text-[10px]">{t(lang, 'updateRestart')}</span>
      </div>
    );
  }

  if (status === 'up-to-date' && showUpToDate) {
    return (
      <div className="flex items-center gap-1.5 px-1.5">
        <span className="text-text-muted text-[10px]">{t(lang, 'updateUpToDate')}</span>
      </div>
    );
  }

  // auto 失败静默 — 仅手动检查失败显示警告图标
  if (status === 'error' && lastTrigger === 'manual') {
    return (
      <div className="flex items-center gap-1.5 px-1.5" title={error ?? t(lang, 'updateError')}>
        <AlertTriangle size={12} className="text-yellow" />
      </div>
    );
  }

  return null;
}
