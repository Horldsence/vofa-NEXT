import { memo, useEffect } from 'react';
import clsx from 'clsx';
import { ChevronRight } from 'lucide-react';
import type { WidgetCategory } from '../../../types';
import { dockDrag } from '../../../lib/dockDrag';
import type { PaletteEntry } from './paletteModel';
import { HEADER_SIZE, ROW_SIZE } from './paletteModel';
import { activateOnKeyboard } from '../../../lib/utils/a11y';

/// 各类别的图标底色 (静态类名, 保证 Tailwind 可扫描)
const categoryTileClass: Record<WidgetCategory, string> = {
  transport: 'bg-yellow/15 text-yellow group-hover:bg-yellow/25',
  protocol: 'bg-accent/15 text-accent group-hover:bg-accent/25',
  send: 'bg-purple/15 text-purple group-hover:bg-purple/25',
  input: 'bg-blue/15 text-blue group-hover:bg-blue/25',
  display: 'bg-green/15 text-green group-hover:bg-green/25',
  math: 'bg-orange/15 text-orange group-hover:bg-orange/25',
  // string 主题色 #ff8a65 (橙红): 色板机制只支持既有 token, red (#f48771)
  // 是色相最接近的既有类, 且与 math 的 orange (#ce9178) 可区分
  string: 'bg-red/15 text-red group-hover:bg-red/25',
  custom: 'bg-purple/15 text-purple group-hover:bg-purple/25',
};

const tileClass = (cat: WidgetCategory) =>
  clsx(
    'w-6 h-6 rounded-sm flex items-center justify-center flex-shrink-0 [&_svg]:w-4 [&_svg]:h-4 transition-colors',
    categoryTileClass[cat],
  );

interface PaletteRowProps {
  entry: PaletteEntry;
  category: WidgetCategory;
  /// 分组刚展开 — 播放入场动画 (淡入 + 轻微下滑)
  entering: boolean;
  /// 分组折叠中 — 播放退场动画 (淡出 + 轻微左移), 结束后行才从模型剔除
  exiting: boolean;
  onActivate: (entry: PaletteEntry) => void;
  /// 入场动画已播放标记 — 虚拟列表滚动会卸载/重挂载行, 标记后重挂载不再重放
  onEnterPlayed: (key: string) => void;
}

/// 控件行 — 单行: 分类色图标块 + 名称, 左键拖拽或单击均可添加
/// 字号取 theme token (--font-size-sm); memo 化后滚动重渲染仅更新可见行
/// 行高用 px 硬编码并与 paletteModel.ROW_SIZE 严格一致: 根字号为 13px,
/// rem 类 (h-8/mb-0.5) 的实际像素与虚拟列表的行高估算不符, 会造成跳转落点累积偏差
export const PaletteRow = memo(function PaletteRow({ entry, category, entering, exiting, onActivate, onEnterPlayed }: PaletteRowProps) {
  /// 仅挂载时标记一次: 播过入场动画的行, 滚动重挂载后不再重放
  useEffect(() => {
    if (entering) onEnterPlayed(entry.key);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div
      className={clsx(
        'group flex items-center gap-2 px-1.5 rounded-sm cursor-grab select-none transition-colors duration-150 active:cursor-grabbing hover:bg-bg-hover',
        entering && 'animate-palette-row',
        exiting && 'animate-palette-row-exit',
      )}
      style={{ height: ROW_SIZE - 2, marginBottom: 2 }}
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        if ((e.target as HTMLElement).closest('button, input')) return;
        dockDrag.begin(e, {
          kind: 'widget',
          widget: {
            kind: entry.kind,
            op: entry.op,
            preset: entry.preset,
            globalNode: entry.globalNode,
            transportKind: entry.transportKind,
          },
          label: entry.label,
        });
      }}
      onClick={() => {
        if (dockDrag.consumeClick()) return;
        onActivate(entry);
      }}
      onKeyDown={activateOnKeyboard}
      role="button"
      tabIndex={0}
      title={entry.title}
    >
      <div className={tileClass(category)}>{entry.icon}</div>
      <span className="text-[length:var(--font-size-sm)] leading-none truncate text-text-secondary transition-colors group-hover:text-text-primary">
        {entry.label}
      </span>
    </div>
  );
});

interface SectionHeaderProps {
  header: string;
  collapsed: boolean;
  onToggle: () => void;
}

/// 分组头 — 点击折叠/展开, chevron 随状态旋转
/// 高度同样用 px 硬编码并与 paletteModel.HEADER_SIZE 严格一致 (原因见 PaletteRow)
export const SectionHeader = memo(function SectionHeader({ header, collapsed, onToggle }: SectionHeaderProps) {
  return (
    <button
      className="flex items-center gap-1 w-full px-1 text-[length:var(--font-size-xs)] font-medium uppercase tracking-wider text-text-disabled hover:text-text-secondary cursor-pointer select-none transition-colors"
      style={{ height: HEADER_SIZE - 2, marginBottom: 2 }}
      onClick={onToggle}
    >
      <ChevronRight
        size={12}
        className={clsx('flex-shrink-0 transition-transform duration-150', !collapsed && 'rotate-90')}
      />
      {header}
    </button>
  );
});
