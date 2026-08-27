//! 操作历史面板 — 数据面板的独立类型 (operation-history)
//!
//! 步骤导航式列表: 最新在最上, 行首为「步骤号 + 节点主题徽章」,
//! 时间戳常显。当前生效条目高亮; 其上方的灰显分区是已被撤销的未来,
//! 中间由「已撤销 · 可重做」分隔条标记游标位置 — 点击任意一条直接
//! 跳转到该时刻 (快照式回滚), 之后的新操作会丢弃其后方分支。
//!
//! 徽章视觉与画布节点同源 (nodeKindVisuals):
//! 控件按分类色 (输入蓝/显示绿/数学橙/字符串红/自定义紫),
//! Transport 黄 · Cable, Protocol 主题色 · Binary;
//! 连线类条目渲染「源色点 → 目标点」双端点配色。

import { Fragment, memo, useEffect, useMemo, useState } from 'react';
import {
  ArrowRight,
  Boxes,
  FileUp,
  Flag,
  History as HistoryIcon,
  MoveRight,
  PanelsTopLeft,
  Redo2,
  Trash2,
  Undo2,
  type LucideIcon,
} from 'lucide-react';
import clsx from 'clsx';
import { useAppStore } from '../../../store/appStore';
import { useHistoryStore, beginHistoryOp, type HistoryEntry } from '../../../store/historyStore';
import { NEUTRAL_VISUAL, nodeVisualOf } from '../../../lib/utils/nodeKindVisuals';
import { t, type Lang } from '../../../i18n';

/// 清空确认的自动复位时长 (ms)
const CONFIRM_RESET_MS = 3000;

function formatTime(ts: number): string {
  return new Date(ts).toLocaleTimeString(undefined, {
    hour12: false,
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}

/// 进度文案 (t() 无插值支持, 数字在代码侧拼装)
function stepProgress(lang: Lang, index: number, total: number): string {
  const nums = `${index + 1}/${total}`;
  return lang === 'zh' ? `第 ${nums} 步` : `Step ${nums}`;
}

interface BadgeVisual {
  Icon: LucideIcon | null;
  tileCls: string;
  /** 连线双端点的圆点配色 [源, 目标] — 存在时优先于图标渲染 */
  dots?: [string, string];
}

/** 条目目标 → 行首徽章 (与画布节点同款主题色) */
function badgeOf(entry: HistoryEntry): BadgeVisual {
  const target = entry.target;
  if (!target) {
    // 无 target 的键级兜底: 基线 / 工作区级操作
    if (entry.opKey === 'opHistoryInitial') {
      return { Icon: Flag, tileCls: NEUTRAL_VISUAL.tileCls };
    }
    if (entry.opKey === 'opImportWorkspace' || entry.opKey === 'opApplyTemplate') {
      return { Icon: FileUp, tileCls: NEUTRAL_VISUAL.tileCls };
    }
    return { Icon: null, tileCls: NEUTRAL_VISUAL.tileCls };
  }
  switch (target.kind) {
    case 'node': {
      const v = nodeVisualOf(target.node);
      return { Icon: v.Icon, tileCls: v.tileCls };
    }
    case 'edge': {
      // 双端点配色: 源色点 → 目标色点; 两端都未知时退化为中性箭头图标
      const fromDot = target.from ? nodeVisualOf(target.from).dotCls : 'bg-border-subtle';
      const toDot = target.to ? nodeVisualOf(target.to).dotCls : 'bg-border-subtle';
      if (!target.from && !target.to) {
        return { Icon: ArrowRight, tileCls: NEUTRAL_VISUAL.tileCls };
      }
      return { Icon: null, tileCls: NEUTRAL_VISUAL.tileCls, dots: [fromDot, toDot] };
    }
    case 'nodes':
      return { Icon: Boxes, tileCls: NEUTRAL_VISUAL.tileCls };
    case 'tab':
      return { Icon: PanelsTopLeft, tileCls: NEUTRAL_VISUAL.tileCls };
    case 'doc':
      return { Icon: FileUp, tileCls: NEUTRAL_VISUAL.tileCls };
  }
}

export const OperationHistoryView = memo(function OperationHistoryView() {
  const lang = useAppStore((s) => s.lang);
  const entries = useHistoryStore((s) => s.entries);
  const index = useHistoryStore((s) => s.index);
  const canUndo = useHistoryStore((s) => s.canUndo);
  const canRedo = useHistoryStore((s) => s.canRedo);
  const undo = useHistoryStore((s) => s.undo);
  const redo = useHistoryStore((s) => s.redo);
  const jumpTo = useHistoryStore((s) => s.jumpTo);
  const clearHistory = useHistoryStore((s) => s.clearHistory);

  const [confirmClear, setConfirmClear] = useState(false);

  // 打开面板即建立基线快照 (无需等首次操作)
  useEffect(() => {
    beginHistoryOp();
  }, []);

  // 两段式清空确认 — 3 秒未二次点击自动复位
  useEffect(() => {
    if (!confirmClear) return;
    const timer = setTimeout(() => setConfirmClear(false), CONFIRM_RESET_MS);
    return () => clearTimeout(timer);
  }, [confirmClear]);

  // 最新在上
  const ordered = useMemo(
    () =>
      entries
        .map((entry, idx) => ({ entry, idx }))
        .reverse(),
    [entries]
  );
  /// 游标之上的未来条目数 (已撤销, 点击可重做)
  const futureCount = useMemo(() => ordered.filter(({ idx }) => idx > index).length, [ordered, index]);

  return (
    <div className="flex flex-col h-full w-full overflow-hidden" data-tour="operation-history">
      {/* 头部: 标题 + 步骤进度 + 撤销/重做/清空 */}
      <div className="flex items-center gap-2 px-2.5 h-9 border-b border-border-subtle bg-bg-panel-header shrink-0">
        <HistoryIcon size={13} className="text-text-secondary shrink-0" />
        <span className="text-[11px] font-semibold uppercase tracking-wider text-text-primary truncate">
          {t(lang, 'operationHistoryTitle')}
        </span>
        {entries.length > 0 && (
          <span
            title={t(lang, 'opHistoryCurrentPosition')}
            className="inline-flex items-center h-5 px-1.5 rounded-full bg-bg-input border border-border-subtle text-[10px] font-mono text-text-secondary tabular-nums"
          >
            {stepProgress(lang, index, entries.length)}
          </span>
        )}
        <div className="ml-auto flex items-center gap-0.5">
          <button type="button" disabled={!canUndo} onClick={() => undo()} title={`${t(lang, 'menuUndo')} (Ctrl+Z)`} className={iconBtnCls}>
            <Undo2 size={14} />
          </button>
          <button type="button" disabled={!canRedo} onClick={() => redo()} title={`${t(lang, 'menuRedo')} (Ctrl+Y)`} className={iconBtnCls}>
            <Redo2 size={14} />
          </button>
          <button
            type="button"
            disabled={entries.length <= 1}
            onClick={() => {
              if (confirmClear) {
                setConfirmClear(false);
                clearHistory();
              } else {
                setConfirmClear(true);
              }
            }}
            title={t(lang, confirmClear ? 'opHistoryClearConfirm' : 'opHistoryClear')}
            className={clsx(iconBtnCls, confirmClear && 'text-red-400 hover:text-red-300')}
          >
            <Trash2 size={14} />
          </button>
        </div>
      </div>

      {/* 列表: 有记录时贴顶铺满滚动; 空状态在面板内水平垂直居中 */}
      <div className="flex-1 min-h-0 overflow-y-auto overflow-x-hidden">
        {ordered.length === 0 ? (
          <div className="h-full w-full flex items-center justify-center">
            <div className="text-text-secondary text-sm select-none">{t(lang, 'opHistoryEmpty')}</div>
          </div>
        ) : (
          <div className="flex flex-col gap-px p-1.5">
            {ordered.map(({ entry, idx }, rowIdx) => {
              const isCurrent = idx === index;
              const isFuture = idx > index;
              const detail = entry.detailKey ? t(lang, entry.detailKey) : entry.detailText ?? '';
              const badge = badgeOf(entry);
              // 游标分隔条紧跟最后一条「未来」行之后
              const showDivider = rowIdx === futureCount - 1;

              return (
                <Fragment key={entry.id}>
                  <button
                    type="button"
                    onClick={() => jumpTo(entry.id)}
                    title={`${formatTime(entry.time)}${detail ? ` · ${detail}` : ''}`}
                    className={clsx(
                      'group flex items-center gap-2 px-2 h-8 rounded-md text-left text-[12px] transition-colors',
                      isCurrent
                        ? 'bg-bg-hover ring-1 ring-accent/60 text-text-bright'
                        : isFuture
                          ? 'opacity-45 hover:opacity-75'
                          : 'hover:bg-bg-input/70 text-text-secondary'
                    )}
                  >
                    <span
                      className={clsx(
                        'w-4 shrink-0 text-right font-mono text-[10px] tabular-nums',
                        isCurrent ? 'text-accent font-bold' : 'text-text-secondary/70'
                      )}
                    >
                      {isCurrent ? '▸' : idx}
                    </span>
                    <span
                      className={clsx(
                        'size-6 shrink-0 rounded flex items-center justify-center gap-[3px]',
                        badge.tileCls
                      )}
                    >
                      {badge.dots ? (
                        <>
                          <i className={clsx('block size-1.5 rounded-full', badge.dots[0])} />
                          <MoveRight size={9} className="shrink-0 opacity-70" />
                          <i className={clsx('block size-1.5 rounded-full', badge.dots[1])} />
                        </>
                      ) : (
                        badge.Icon && <badge.Icon size={13} />
                      )}
                    </span>
                    <span className="min-w-0 flex-1 truncate">
                      {t(lang, entry.opKey)}
                      {detail && <span className="text-text-secondary"> · {detail}</span>}
                    </span>
                    <span className="shrink-0 font-mono text-[10px] text-text-secondary tabular-nums">
                      {formatTime(entry.time)}
                    </span>
                  </button>

                  {showDivider && (
                    <div className="flex items-center gap-2 px-3 py-1.5 my-0.5" aria-hidden>
                      <span className="h-px flex-1 bg-border-subtle" />
                      <span className="text-[10px] tracking-wide text-text-secondary whitespace-nowrap">
                        ▲ {t(lang, 'opHistoryRedoZone')}
                      </span>
                      <span className="h-px flex-1 bg-border-subtle" />
                    </div>
                  )}
                </Fragment>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
});

const iconBtnCls =
  'flex items-center justify-center size-6 rounded text-text-secondary transition-colors hover:bg-bg-hover hover:text-text-bright disabled:opacity-35 disabled:hover:bg-transparent disabled:hover:text-text-secondary disabled:cursor-not-allowed';
