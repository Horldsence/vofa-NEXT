import { memo } from 'react';
import { useAppStore } from '../../store/appStore';
import { Loader2, AlertTriangle } from 'lucide-react';
import clsx from 'clsx';

/// 状态栏 / Tab 顶端都会用到的编译状态指示元素 (Dots + 文案).
/// 单一权威 — 状态来源于 `useAppStore.tabStates[tabId]`,
/// 显示模式:
/// - compiling / pending: 黄色 Loader + "Compiling..."
/// - error: 红色 AlertTriangle + 错误 tab 数角标
/// - ok: 隐藏 (避免状态栏噪音)
const CompileStatusIndicator = memo(function CompileStatusIndicator({
  tabId,
  compact = false,
}: {
  tabId?: string;
  compact?: boolean;
}) {
  const lang = useAppStore((s) => s.lang);
  const errorTabs = useAppStore((s) => s.errorTabs);
  const pendingTabs = useAppStore((s) => s.pendingTabs);
  const tabState = useAppStore((s) => (tabId ? s.tabStates[tabId] : undefined));
  void lang;

  const scope: { state: 'ok' | 'pending' | 'compiling' | 'error'; count: number } = tabId
    ? {
        state: tabState ?? 'ok',
        count: tabState === 'error' ? 1 : pendingTabs.includes(tabId) ? 1 : 0,
      }
    : {
        state: pendingTabs.length > 0
          ? 'compiling'
          : errorTabs.length > 0
          ? 'error'
          : 'ok',
        count: errorTabs.length,
      };

  if (scope.state === 'ok') return null;

  if (scope.state === 'error') {
    return (
      <span
        className={clsx(
          'flex items-center gap-1 text-red-500',
          compact ? 'h-full whitespace-nowrap' : 'h-full whitespace-nowrap px-1',
        )}
        title="Compile errors"
      >
        <AlertTriangle size={12} />
        {!compact && (
          <span className="tabular-nums">{scope.count}</span>
        )}
      </span>
    );
  }

  // pending / compiling
  return (
    <span
      className="flex items-center gap-1 text-yellow-500 h-full whitespace-nowrap"
      title="Compiling"
    >
      <Loader2 size={12} className="animate-spin" />
      {!compact && <span>Compiling...</span>}
    </span>
  );
});

export default CompileStatusIndicator;

// 单 tab id 模式 (供 StatusBar 与 Tab 头部复用)
export function CompileDot({ tabId }: { tabId: string }) {
  const state = useAppStore((s) => s.tabStates[tabId]);
  if (!state) return null;
  if (state === 'ok') return null;
  return (
    <span
      className={clsx(
        'inline-block h-1.5 w-1.5 rounded-full flex-shrink-0',
        state === 'error'
          ? 'bg-red-500'
          : 'bg-yellow-500 animate-pulse',
      )}
      title={state === 'error' ? 'Compile error' : 'Compiling'}
    />
  );
}
