import { create } from 'zustand';
import { persist, createJSONStorage } from 'zustand/middleware';
import { useAppStore } from './appStore';

/// 卡片四向吸附边
export type SnapEdge = 'top' | 'bottom' | 'left' | 'right';

/// 分栏方向: row = 左右, col = 上下
export type DockDirection = 'row' | 'col';
/// 卡片种类 — 决定可承载的 Tab 类型 (控件画布 / 数据视图)
/// 仅「合并进同一 Tab 条」限制同 kind; 边缘拆分/页面级停靠允许跨 kind
export type CardKind = 'control' | 'data';

export interface DockCard {
  id: string;
  kind: CardKind;
  tabIds: string[];
  activeTabId: string | null;
}

/// 布局树: N 叉 split (sizes 与 children 一一对应, 百分比) + 卡片叶子
export type DockNode =
  | { id: string; type: 'card'; cardId: string }
  | { id: string; type: 'split'; dir: DockDirection; children: DockNode[]; sizes: number[] };

interface LegacyDockCardNode {
  id: string;
  type: 'card';
  cardId: string;
}

interface LegacyDockSplitNode {
  id: string;
  type: 'split';
  dir: DockDirection;
  a: LegacyDockNode;
  b: LegacyDockNode;
  ratio?: number;
}

type LegacyDockNode = LegacyDockCardNode | LegacyDockSplitNode;

function isLegacyDockNode(value: unknown): value is LegacyDockNode {
  if (typeof value !== 'object' || value === null || !('type' in value)) return false;
  if (value.type === 'card') {
    return 'id' in value && typeof value.id === 'string' && 'cardId' in value && typeof value.cardId === 'string';
  }
  return (
    value.type === 'split' &&
    'id' in value &&
    typeof value.id === 'string' &&
    'dir' in value &&
    (value.dir === 'row' || value.dir === 'col') &&
    'a' in value &&
    'b' in value &&
    isLegacyDockNode(value.a) &&
    isLegacyDockNode(value.b) &&
    (!('ratio' in value) || typeof value.ratio === 'number')
  );
}

export interface DragTabPayload {
  kind: CardKind;
  tabId: string;
  fromCardId: string;
}

/// 当前悬停的投放目标 — cardId 为 null 表示页面级边缘热区
export interface DropTarget {
  cardId: string | null;
  edge: SnapEdge;
}

/// edge 对应的分栏轴向
export function edgeDir(edge: SnapEdge): DockDirection {
  return edge === 'left' || edge === 'right' ? 'row' : 'col';
}

/// 新节点是否排在 target 之前 (上/左)
function isNewFirst(edge: SnapEdge): boolean {
  return edge === 'left' || edge === 'top';
}

let idCounter = 0;
function nextId(prefix: string): string {
  idCounter += 1;
  return `${prefix}-${Date.now().toString(36)}-${idCounter}`;
}

/// 从树中摘除指定卡片节点, 折叠空 split / 单子 split; 返回新树 (null = 树已空)
/// 导出供 DockLayout 计算"摘除后的预览几何"
export function removeCardNode(node: DockNode, cardId: string): DockNode | null {
  if (node.type === 'card') return node.cardId === cardId ? null : node;
  const keptChildren: DockNode[] = [];
  const keptSizes: number[] = [];
  let changed = false;
  node.children.forEach((child, i) => {
    const next = removeCardNode(child, cardId);
    if (next !== child) changed = true;
    if (next === null) return;
    keptChildren.push(next);
    keptSizes.push(node.sizes[i] ?? 100 / node.children.length);
  });
  if (!changed) return node;
  if (keptChildren.length === 0) return null;
  if (keptChildren.length === 1) return keptChildren[0];
  return { ...node, children: keptChildren, sizes: keptSizes };
}

/// 在 target 卡片的 edge 方位插入 newNode
/// - target 的父 split 轴向与 edge 匹配 → 作为同级条带插入 (偷取 target 一半份额)
/// - 否则 → 把 target 包装成一个新的垂直轴向 split (对半分)
function insertAtEdge(node: DockNode, targetCardId: string, edge: SnapEdge, newNode: DockNode): DockNode {
  if (node.type === 'card') {
    if (node.cardId !== targetCardId) return node;
    const first = isNewFirst(edge);
    return {
      id: nextId('split'),
      type: 'split',
      dir: edgeDir(edge),
      children: first ? [newNode, node] : [node, newNode],
      sizes: [50, 50],
    };
  }
  // target 是本 split 的直接子节点且轴向匹配 → 同级插入
  const idx = node.children.findIndex((c) => c.type === 'card' && c.cardId === targetCardId);
  if (idx >= 0 && node.dir === edgeDir(edge)) {
    const children = node.children.slice();
    const sizes = node.sizes.slice();
    const targetSize = sizes[idx] ?? 100 / children.length;
    const newSize = targetSize / 2;
    sizes[idx] = targetSize - newSize;
    const at = isNewFirst(edge) ? idx : idx + 1;
    children.splice(at, 0, newNode);
    sizes.splice(at, 0, newSize);
    return { ...node, children, sizes };
  }
  let changed = false;
  const children = node.children.map((c) => {
    const next = insertAtEdge(c, targetCardId, edge, newNode);
    if (next !== c) changed = true;
    return next;
  });
  return changed ? { ...node, children } : node;
}

/// 页面级边缘插入: 根 split 轴向匹配 → 追加为整行/整列条带 (占 25%); 否则包装根节点
function insertAtRootEdge(root: DockNode, edge: SnapEdge, newNode: DockNode): DockNode {
  const dir = edgeDir(edge);
  const first = isNewFirst(edge);
  const share = 25;
  if (root.type === 'split' && root.dir === dir) {
    const scale = (100 - share) / 100;
    const sizes = root.sizes.map((s) => s * scale);
    return {
      ...root,
      children: first ? [newNode, ...root.children] : [...root.children, newNode],
      sizes: first ? [share, ...sizes] : [...sizes, share],
    };
  }
  return {
    id: nextId('split'),
    type: 'split',
    dir,
    children: first ? [newNode, root] : [root, newNode],
    sizes: first ? [share, 100 - share] : [100 - share, share],
  };
}

function updateSplitSizes(node: DockNode, splitId: string, sizes: number[]): DockNode {
  if (node.type === 'card') return node;
  if (node.id === splitId) return { ...node, sizes };
  let changed = false;
  const children = node.children.map((c) => {
    const next = updateSplitSizes(c, splitId, sizes);
    if (next !== c) changed = true;
    return next;
  });
  return changed ? { ...node, children } : node;
}

/// 同步全局"最近激活 Tab" (供菜单/控件面板等既有逻辑使用)
function mirrorActiveTab(kind: CardKind, tabId: string | null) {
  if (!tabId) return;
  const s = useAppStore.getState();
  if (kind === 'control') s.setActiveControlTab(tabId);
  else s.setActiveDataTab(tabId);
}

interface DockState {
  root: DockNode;
  cards: Record<string, DockCard>;
  /// 最近交互的卡片 — 新建 Tab 的落点
  focusedCardId: string | null;
  /// 正在拖拽的 Tab (不持久化)
  draggingTab: DragTabPayload | null;
  /// 正在整卡拖拽的卡片 (不持久化)
  draggingCardId: string | null;
  /// 当前悬停的投放目标 (不持久化) — 驱动全局预览
  dropTarget: DropTarget | null;
  /// 指针悬停的合并目标卡片 (不持久化) — 标题栏 Tab 合并高亮
  mergeHoverCardId: string | null;
  setActiveTab: (cardId: string, tabId: string) => void;
  setFocusedCard: (cardId: string) => void;
  setSizes: (splitId: string, sizes: number[]) => void;
  setDraggingTab: (d: DragTabPayload | null) => void;
  setDraggingCard: (cardId: string | null) => void;
  setDropTarget: (t: DropTarget | null) => void;
  setMergeHover: (cardId: string | null) => void;
  /// 把当前拖拽的 Tab 合并进目标卡片 (仅同 kind)
  moveTabToCard: (targetCardId: string) => void;
  /// 在当前拖拽的 Tab / 卡片落到目标卡片的某个方位 (同级条带插入或拆分)
  dropOnCardEdge: (targetCardId: string, edge: SnapEdge) => void;
  /// 当前拖拽的 Tab / 卡片落到页面边缘 (整行/整列条带)
  dropOnRootEdge: (edge: SnapEdge) => void;
  /// 与 appStore 的 Tab 列表对账: 剔除已删除的 Tab, 安置新增 Tab, 裁剪空卡片
  reconcile: (kind: CardKind, existingTabIds: string[]) => void;
}

const defaultCards: Record<string, DockCard> = {
  'control-main': { id: 'control-main', kind: 'control', tabIds: [], activeTabId: null },
  'data-main': { id: 'data-main', kind: 'data', tabIds: [], activeTabId: null },
};

const defaultRoot: DockNode = {
  id: 'split-root',
  type: 'split',
  dir: 'col',
  children: [
    { id: 'node-control', type: 'card', cardId: 'control-main' },
    { id: 'node-data', type: 'card', cardId: 'data-main' },
  ],
  sizes: [45, 55],
};

/// 从拖拽状态构建结果: 摘出 Tab 生成新卡片 / 摘除整卡, 返回新的 root+cards+待插入节点
function extractDraggedNode(state: Pick<DockState, 'root' | 'cards' | 'draggingTab' | 'draggingCardId'>): {
  root: DockNode | null;
  cards: Record<string, DockCard>;
  node: DockNode;
  focusedCardId: string;
} | null {
  const { root, cards, draggingTab: d, draggingCardId } = state;
  if (d) {
    const origin = cards[d.fromCardId];
    if (!origin?.tabIds.includes(d.tabId)) return null;
    const originTabs = origin.tabIds.filter((id) => id !== d.tabId);
    const newCardId = nextId('card');
    const nextCards: Record<string, DockCard> = {
      ...cards,
      [origin.id]: {
        ...origin,
        tabIds: originTabs,
        activeTabId: origin.activeTabId === d.tabId ? (originTabs[0] ?? null) : origin.activeTabId,
      },
      [newCardId]: { id: newCardId, kind: d.kind, tabIds: [d.tabId], activeTabId: d.tabId },
    };
    let nextRoot: DockNode | null = root;
    if (originTabs.length === 0) {
      nextRoot = removeCardNode(root, origin.id);
      delete nextCards[origin.id];
    }
    mirrorActiveTab(d.kind, d.tabId);
    return {
      root: nextRoot,
      cards: nextCards,
      node: { id: nextId('node'), type: 'card', cardId: newCardId },
      focusedCardId: newCardId,
    };
  }
  if (draggingCardId) {
    if (!cards[draggingCardId]) return null;
    return {
      root: removeCardNode(root, draggingCardId),
      cards,
      node: { id: nextId('node'), type: 'card', cardId: draggingCardId },
      focusedCardId: draggingCardId,
    };
  }
  return null;
}

export const useDockStore = create<DockState>()(
  persist(
    (set) => ({
      root: defaultRoot,
      cards: defaultCards,
      focusedCardId: null,
      draggingTab: null,
      draggingCardId: null,
      dropTarget: null,
      mergeHoverCardId: null,

      setActiveTab: (cardId, tabId) =>
        set((state) => {
          const card = state.cards[cardId];
          if (!card?.tabIds.includes(tabId)) return state;
          mirrorActiveTab(card.kind, tabId);
          return {
            cards: { ...state.cards, [cardId]: { ...card, activeTabId: tabId } },
            focusedCardId: cardId,
          };
        }),

      setFocusedCard: (cardId) =>
        set((state) => {
          const card = state.cards[cardId];
          if (!card || state.focusedCardId === cardId) return state;
          mirrorActiveTab(card.kind, card.activeTabId);
          return { focusedCardId: cardId };
        }),

      setSizes: (splitId, sizes) =>
        set((state) => ({ root: updateSplitSizes(state.root, splitId, sizes) })),

      setDraggingTab: (draggingTab) => set({ draggingTab }),
      setDraggingCard: (draggingCardId) => set({ draggingCardId }),

      setDropTarget: (dropTarget) =>
        set((state) => {
          const cur = state.dropTarget;
          if (cur === dropTarget) return state;
          if (cur && dropTarget && cur.cardId === dropTarget.cardId && cur.edge === dropTarget.edge) {
            return state;
          }
          return { dropTarget };
        }),

      setMergeHover: (mergeHoverCardId) =>
        set((state) => (state.mergeHoverCardId === mergeHoverCardId ? state : { mergeHoverCardId })),

      moveTabToCard: (targetCardId) =>
        set((state) => {
          const d = state.draggingTab;
          if (!d) return state;
          const origin = state.cards[d.fromCardId];
          const target = state.cards[targetCardId];
          if (!origin || !target || origin.id === target.id || origin.kind !== target.kind) {
            return { draggingTab: null };
          }
          const originTabs = origin.tabIds.filter((id) => id !== d.tabId);
          const cards: Record<string, DockCard> = {
            ...state.cards,
            [origin.id]: {
              ...origin,
              tabIds: originTabs,
              activeTabId: origin.activeTabId === d.tabId ? (originTabs[0] ?? null) : origin.activeTabId,
            },
            [target.id]: { ...target, tabIds: [...target.tabIds, d.tabId], activeTabId: d.tabId },
          };
          let root = state.root;
          if (originTabs.length === 0) {
            const nextRoot = removeCardNode(root, origin.id);
            if (nextRoot) {
              root = nextRoot;
              delete cards[origin.id];
            }
          }
          mirrorActiveTab(target.kind, d.tabId);
          return { root, cards, draggingTab: null, focusedCardId: target.id };
        }),

      dropOnCardEdge: (targetCardId, edge) =>
        set((state) => {
          // 单 Tab 卡片拖到自身边缘无意义
          const d = state.draggingTab;
          if (d && d.fromCardId === targetCardId) {
            const origin = state.cards[d.fromCardId];
            if (!origin || origin.tabIds.length <= 1) return { draggingTab: null, dropTarget: null };
          }
          const extracted = extractDraggedNode(state);
          if (!extracted?.root) return { draggingTab: null, draggingCardId: null, dropTarget: null };
          const root = insertAtEdge(extracted.root, targetCardId, edge, extracted.node);
          return {
            root,
            cards: extracted.cards,
            draggingTab: null,
            draggingCardId: null,
            dropTarget: null,
            focusedCardId: extracted.focusedCardId,
          };
        }),

      dropOnRootEdge: (edge) =>
        set((state) => {
          const extracted = extractDraggedNode(state);
          if (!extracted) return { draggingTab: null, draggingCardId: null, dropTarget: null };
          // 树被摘空 (唯一卡片/Tab 被拖出) → 新节点即整棵树
          const root = extracted.root
            ? insertAtRootEdge(extracted.root, edge, extracted.node)
            : extracted.node;
          return {
            root,
            cards: extracted.cards,
            draggingTab: null,
            draggingCardId: null,
            dropTarget: null,
            focusedCardId: extracted.focusedCardId,
          };
        }),

      reconcile: (kind, existingTabIds) =>
        set((state) => {
          const existing = new Set(existingTabIds);
          const cards = { ...state.cards };
          let root = state.root;
          let changed = false;

          // 1. 剔除已删除的 Tab
          for (const card of Object.values(cards)) {
            if (card.kind !== kind) continue;
            const kept = card.tabIds.filter((id) => existing.has(id));
            if (kept.length !== card.tabIds.length) {
              changed = true;
              cards[card.id] = {
                ...card,
                tabIds: kept,
                activeTabId: card.activeTabId && kept.includes(card.activeTabId) ? card.activeTabId : (kept[0] ?? null),
              };
            }
          }

          // 2. 空卡片裁剪 (该 kind 还有其他卡片时)
          for (const card of Object.values(cards)) {
            if (card.kind !== kind || card.tabIds.length > 0) continue;
            const hasSibling = Object.values(cards).some((c) => c.kind === kind && c.id !== card.id);
            if (!hasSibling) continue;
            const nextRoot = removeCardNode(root, card.id);
            if (nextRoot) {
              root = nextRoot;
              delete cards[card.id];
              changed = true;
            }
          }

          // 3. 安置新增 Tab → 焦点卡片 / 该 kind 首张卡片 / 新建卡片
          const housed = new Set(
            Object.values(cards)
              .filter((c) => c.kind === kind)
              .flatMap((c) => c.tabIds)
          );
          const missing = existingTabIds.filter((id) => !housed.has(id));
          if (missing.length > 0) {
            changed = true;
            let target =
              Object.values(cards).find((c) => c.kind === kind && c.id === state.focusedCardId) ??
              Object.values(cards).find((c) => c.kind === kind);
            if (!target) {
              const id = nextId('card');
              target = { id, kind, tabIds: [], activeTabId: null };
              cards[id] = target;
              const node: DockNode = { id: nextId('node'), type: 'card', cardId: id };
              root = { id: nextId('split'), type: 'split', dir: 'col', children: [root, node], sizes: [70, 30] };
            }
            const cur = cards[target.id];
            cards[target.id] = {
              ...cur,
              tabIds: [...cur.tabIds, ...missing],
              activeTabId: missing[missing.length - 1],
            };
          }

          if (!changed) return state;
          return { root, cards };
        }),
    }),
    {
      name: 'vofa-dock',
      version: 2,
      storage: createJSONStorage(() => localStorage),
      partialize: (s) => ({ root: s.root, cards: s.cards, focusedCardId: s.focusedCardId }),
      // v1 → v2: 二叉 split {a, b, ratio} → N 叉 {children, sizes}
      migrate: (persisted: unknown, version: number) => {
        const state = persisted as { root?: unknown; cards?: unknown; focusedCardId?: string | null };
        if (version < 2 && isLegacyDockNode(state.root)) {
          const conv = (n: LegacyDockNode): DockNode =>
            n.type === 'card'
              ? n
              : {
                  id: n.id,
                  type: 'split',
                  dir: n.dir,
                  children: [conv(n.a), conv(n.b)],
                  sizes: [n.ratio ?? 50, 100 - (n.ratio ?? 50)],
                };
          state.root = conv(state.root);
        }
        return state;
      },
    }
  )
);
