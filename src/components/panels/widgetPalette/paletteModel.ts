import type { ReactNode } from 'react';
import type { WidgetConfig, WidgetCategory, MathOp, FilterPresetKind, TransportConfig } from '../../../types';

/// 面板项统一模型 — 各分类项归一成同构条目, 渲染走同一套行样式
export interface PaletteEntry {
  key: string;
  kind?: WidgetConfig['kind'];
  icon: ReactNode;
  label: string;
  op?: MathOp;
  preset?: FilterPresetKind;
  /// 全局节点条目: 数据接口 / 协议引擎 (拖入或点击创建全局节点)
  globalNode?: 'transport' | 'protocol';
  transportKind?: TransportConfig['kind'];
  onAdd?: () => void;
  title: string;
}

export type SectionId = 'input' | 'transport' | 'protocol' | 'display' | 'math' | 'filter' | 'fft' | 'custom';

export interface PaletteSection {
  id: SectionId;
  header: string;
  /// 图标块 / 跳转条所用分类色
  category: WidgetCategory;
  entries: PaletteEntry[];
}

/// 虚拟列表扁平条目 — 分组头与控件行统一进一个数组, 由 useVirtualizer 驱动
export type FlatItem =
  | { type: 'header'; key: string; sectionId: SectionId; header: string; category: WidgetCategory }
  | { type: 'row'; key: string; sectionId: SectionId; category: WidgetCategory; entry: PaletteEntry };

/// 虚拟列表行高 (px) — 固定行高跳过 measureElement 的 DOM 测量开销
/// 行内容 32 + 2 间距; 分组头内容 24 + 2 间距
export const ROW_SIZE = 34;
export const HEADER_SIZE = 26;

/// sections + 折叠状态 → 扁平条目数组 (折叠分组仅保留分组头)
export function flattenSections(
  sections: readonly PaletteSection[],
  collapsed: Partial<Record<SectionId, boolean>>,
): FlatItem[] {
  const items: FlatItem[] = [];
  for (const section of sections) {
    items.push({
      type: 'header',
      key: `h:${section.id}`,
      sectionId: section.id,
      header: section.header,
      category: section.category,
    });
    if (collapsed[section.id]) continue;
    for (const entry of section.entries) {
      items.push({
        type: 'row',
        key: `r:${entry.key}`,
        sectionId: section.id,
        category: section.category,
        entry,
      });
    }
  }
  return items;
}

/// 分组锚点 — 各分组 header 在列表内容中的像素偏移 (按固定行高累计)
export interface SectionAnchor {
  id: SectionId;
  offset: number;
}

/// 扁平模型 → 各分组的像素锚点 (与 useVirtualizer 的固定行高估算同源, 保证跳转落点精确)
export function sectionAnchors(items: readonly FlatItem[]): SectionAnchor[] {
  const anchors: SectionAnchor[] = [];
  let offset = 0;
  for (const item of items) {
    if (item.type === 'header') anchors.push({ id: item.sectionId, offset });
    offset += item.type === 'header' ? HEADER_SIZE : ROW_SIZE;
  }
  return anchors;
}

/// 列表内容总高度 (px)
export function totalSizeOf(items: readonly FlatItem[]): number {
  let size = 0;
  for (const item of items) size += item.type === 'header' ? HEADER_SIZE : ROW_SIZE;
  return size;
}

/// 滚动位置所属分组 — 取 header 偏移不超过 scrollTop (留 slack 余量) 的最后一个分组
/// slack 与重构前行为一致: 分组头接近可视区顶部一行以内即视为进入该分组
export function sectionAtScroll(
  anchors: readonly SectionAnchor[],
  scrollTop: number,
  slack = 32,
): SectionId {
  let current: SectionId = anchors[0]?.id ?? 'input';
  for (const anchor of anchors) {
    if (anchor.offset <= scrollTop + slack) current = anchor.id;
  }
  return current;
}
