import { memo, type ReactNode } from 'react';
import clsx from 'clsx';
import type { SectionId } from './paletteModel';

/// 跳转条入口 — 图标 + 分类色, 点击平滑滚动到对应分组
export interface JumpTarget {
  id: SectionId;
  label: string;
  color: string;
  icon: ReactNode;
}

interface JumpBarProps {
  targets: JumpTarget[];
  /// 当前高亮入口 (滤波器/频域已由容器归入「算术」)
  activeId: SectionId;
  onJump: (id: SectionId) => void;
}

/// 顶部分类跳转条 — memo 隔离: 高亮切换 / 列表滚动互不重渲染
export const JumpBar = memo(function JumpBar({ targets, activeId, onJump }: JumpBarProps) {
  return (
    <div
      className="flex items-center gap-0.5 p-1 rounded-lg bg-bg-panel-header border border-border-subtle flex-shrink-0"
      data-tour="palette-jumpbar"
    >
      {targets.map((target) => {
        const active = activeId === target.id;
        return (
          <button
            key={target.id}
            title={target.label}
            className={clsx(
              'flex-1 flex items-center justify-center h-7 rounded-sm cursor-pointer transition-colors duration-150 select-none',
              active ? 'bg-bg-hover' : 'hover:bg-bg-hover',
            )}
            onClick={() => onJump(target.id)}
          >
            <span
              className="flex items-center transition-colors"
              style={{ color: active ? target.color : undefined }}
            >
              <span className={active ? '' : 'text-text-secondary'}>{target.icon}</span>
            </span>
          </button>
        );
      })}
    </div>
  );
});
