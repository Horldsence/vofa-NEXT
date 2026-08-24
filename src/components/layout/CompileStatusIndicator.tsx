import { memo } from 'react';
import { useShallow } from 'zustand/react/shallow';
import { useAppStore } from '../../store/appStore';
import { Loader2, AlertTriangle, CheckCircle, Circle } from 'lucide-react';
import clsx from 'clsx';

/// 状态栏 / Tab 顶端都会用到的编译状态指示元素 (Dots + 文案).
/// 单一权威 — 状态来源于 `useAppStore.tabStates[tabId]`,
/// 显示模式:
/// - compiling / pending: 黄色 Loader + "Compiling..."
/// - error: 红色 AlertTriangle + 错误 tab 数角标
/// - ok: 绿色 CheckCircle

export interface CompileStatus {
  state: 'ok' | 'pending' | 'compiling' | 'error';
  count: number;
}

/// 统一编译状态结算接口 — 计算单个 Tab 或全局编译状态/计数
export function getCompileStatus(state: any, tabId?: string): CompileStatus {
  if (tabId) {
    const tabState = state.tabStates[tabId] ?? 'ok';
    const isPending = state.pendingTabs.includes(tabId);
    return {
      state: tabState,
      count: tabState === 'error' ? 1 : isPending ? 1 : 0,
    };
  } else {
    // 全局编译状态: 任何 tab 处于 compiling 时显示 compiling; 否则仅在有 active error 时显示 error; 否则 ok
    const activeErrors = state.errorTabs.filter((id: string) => state.tabStates[id] === 'error');
    const isCompiling = state.pendingTabs.length > 0;
    return {
      state: isCompiling ? 'compiling' : activeErrors.length > 0 ? 'error' : 'ok',
      count: activeErrors.length,
    };
  }
}

const CompileStatusIndicator = memo(function CompileStatusIndicator({
  tabId,
  compact = false,
  onClickError,
}: {
  tabId?: string;
  compact?: boolean;
  /// 状态栏全局 error 态注入: 点击触发新建 Compile Errors 数据 tab.
  /// 单 tab id 模式 (CompileDot) 不传 — 保持纯展示, 避免误触.
  onClickError?: () => void;
}) {
  const lang = useAppStore((s) => s.lang);
  const scope = useAppStore(useShallow((s) => getCompileStatus(s, tabId)));
  void lang;

  if (scope.state === 'ok') {
    const interactive = !!onClickError;
    return (
      <button
        type="button"
        className={clsx(
          'flex items-center gap-1 text-green-500',
          compact ? 'h-full whitespace-nowrap' : 'h-full whitespace-nowrap px-1',
          interactive &&
            'cursor-pointer hover:text-green-400 active:text-green-300 rounded transition-colors duration-150',
        )}
        title="Compile errors"
        aria-label={`${scope.count} compile error${scope.count > 1 ? 's' : ''}`}
        onClick={onClickError}
      >
        <CheckCircle size={12} />
      </button>
    );
  }

  if (scope.state === 'error') {
    // onClickError 注入时切换为 button — 状态栏 tier 收缩不受影响 (className 形态一致)
    const interactive = !!onClickError;
    return (
      <button
        type="button"
        className={clsx(
          'flex items-center gap-1 text-red-500',
          compact ? 'h-full whitespace-nowrap' : 'h-full whitespace-nowrap px-1',
          interactive &&
            'cursor-pointer hover:text-red-400 active:text-red-300 rounded transition-colors duration-150',
        )}
        title="Compile errors"
        aria-label={`${scope.count} compile error${scope.count > 1 ? 's' : ''}`}
        onClick={onClickError}
      >
        <AlertTriangle size={12} />
        {!compact && (
          <span className="tabular-nums">{scope.count}</span>
        )}
      </button>
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
  if (!state || state === 'ok') return null;
  if (state === 'error') {
    return (
      <AlertTriangle
        size={10}
        className="text-red-500 shrink-0"
        aria-label="Compile error"
      />
    );
  }
  if (state === 'compiling') {
    return (
      <Loader2
        size={10}
        className="text-yellow-500 animate-spin shrink-0"
        aria-label="Compiling"
      />
    );
  }
  return (
    <Circle
      size={6}
      className="text-yellow-500 fill-yellow-500 shrink-0"
      aria-label="Compile pending"
    />
  );
}
