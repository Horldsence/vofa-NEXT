import { memo } from 'react';
import { Pause, Play, Square } from 'lucide-react';
import clsx from 'clsx';
import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import type { RunState } from '../../types';

/// 运行状态徽标配色 + 标签
const STATE_BADGE: Record<RunState, { dot: string; label: string; title: string }> = {
  stopped: { dot: 'bg-text-muted', label: 'runStateStopped', title: 'runStartHint' },
  running: { dot: 'bg-green', label: 'runStateRunning', title: 'runPauseHint' },
  paused: { dot: 'bg-yellow', label: 'runStatePaused', title: 'runStartHint' },
};

/// 工作区运行控制 — 启动/暂停/停止 (状态权威在 Rust, 本组件仅触发与镜像)
///
/// 数据链与自动发送只在运行态工作: 新建/重开项目默认停止,
/// 暂停保持设备连接并丢弃期间数据, 停止同时清空待发送任务。
export const RunControls = memo(function RunControls() {
  const lang = useAppStore((s) => s.lang);
  const runState = useAppStore((s) => s.runState);
  const workspaceRun = useAppStore((s) => s.workspaceRun);

  const badge = STATE_BADGE[runState];

  return (
    <div className="flex items-center gap-1 h-full shrink-0" data-testid="run-controls">
      <span className={clsx('w-2 h-2 rounded-full inline-block', badge.dot)} />
      <span className="whitespace-nowrap mr-0.5">{t(lang, badge.label)}</span>
      <button
        className={clsx(
          'w-5 h-5 flex items-center justify-center rounded transition-colors duration-150',
          runState === 'running'
            ? 'text-green bg-green/10'
            : 'text-text-secondary hover:bg-bg-hover hover:text-green'
        )}
        title={t(lang, 'runStartHint')}
        aria-label={t(lang, 'runStartHint')}
        onClick={() => { void workspaceRun('start'); }}
      >
        <Play size={11} />
      </button>
      <button
        className={clsx(
          'w-5 h-5 flex items-center justify-center rounded transition-colors duration-150',
          runState === 'paused'
            ? 'text-yellow bg-yellow/10'
            : 'text-text-secondary hover:bg-bg-hover hover:text-yellow',
          runState === 'stopped' && 'opacity-40 hover:opacity-40 hover:text-text-secondary'
        )}
        title={t(lang, 'runPauseHint')}
        aria-label={t(lang, 'runPauseHint')}
        disabled={runState === 'stopped'}
        onClick={() => { void workspaceRun('pause'); }}
      >
        <Pause size={11} />
      </button>
      <button
        className={clsx(
          'w-5 h-5 flex items-center justify-center rounded transition-colors duration-150',
          runState === 'stopped'
            ? 'text-text-secondary opacity-40 hover:opacity-40 hover:text-text-secondary'
            : 'text-text-secondary hover:bg-bg-hover hover:text-red'
        )}
        title={t(lang, 'runStopHint')}
        aria-label={t(lang, 'runStopHint')}
        disabled={runState === 'stopped'}
        onClick={() => { void workspaceRun('stop'); }}
      >
        <Square size={10} />
      </button>
    </div>
  );
});
