//! 指针事件驱动的 dock 拖拽控制器 (替代 HTML5 Drag & Drop)
//!
//! Tauri (WKWebView) 下 HTML5 `draggable` 拖拽不可靠: 手势消歧经常把拖拽判成
//! 文字选择 ("框选文字"), 或拖拽根本不触发。本模块用 pointer 事件实现拖拽:
//!
//! - 拖拽源在 `pointerdown` 时调用 `begin()` (不 preventDefault, 点击/双击照常)
//! - 指针移动超过阈值 (5px) 才激活拖拽 → 写入 store 的 dragging* 状态 + 显示幽灵
//! - 激活后逐帧命中测试 `[data-dock-zone]` 投放区, 驱动 dropTarget/mergeHover 预览
//! - `pointerup` 提交落点 (合并/拆卡/页面停靠/侧边栏停靠/画布投放), 否则取消
//! - Escape / 窗口失焦 / pointercancel 取消拖拽
//!
//! 投放区通过 data 属性声明 (不依赖事件冒泡):
//! - `data-dock-zone="card-edge"` + data-dock-card + data-dock-kind  → 卡片边缘拆分
//! - `data-dock-zone="merge"`     + data-dock-card + data-dock-kind  → 标题栏 Tab 合并
//! - `data-dock-zone="page-edge"` + data-dock-edge                    → 页面边缘条带
//! - `data-dock-zone="sidebar-dock"` + data-dock-edge                 → 侧边栏停靠
//! - `data-dock-zone="canvas"`                                        → 控件画布投放

import { useDockStore, type CardKind, type DragTabPayload, type SnapEdge } from '../store/dockStore';
import { useLayoutStore, type SidebarDock } from '../store/layoutStore';
import type { FilterPresetKind, MathOp, TransportConfig, WidgetConfig } from '../types';

/// 从控件面板拖出的控件参数 (拖到画布)
export interface WidgetDragSpec {
  /// 控件类型 — 全局节点条目 (globalNode 设置) 时缺省
  kind?: WidgetConfig['kind'];
  op?: MathOp;
  preset?: FilterPresetKind;
  /// 全局节点拖入: 'transport' = 数据接口 (transportKind 指定类型), 'protocol' = 协议引擎
  globalNode?: 'transport' | 'protocol';
  transportKind?: TransportConfig['kind'];
}

export type DockDragSpec =
  | { kind: 'tab'; tab: DragTabPayload; label: string }
  | { kind: 'card'; cardId: string; label: string }
  | { kind: 'sidebar'; label: string }
  | { kind: 'widget'; widget: WidgetDragSpec; label: string };

export interface GhostState {
  x: number;
  y: number;
  label: string;
  /// 释放态 — 拖拽放下后的短暂放大淡出动画; 低动画偏好 (prefers-reduced-motion) 时不产生
  releasing?: boolean;
}

type Hover =
  | { type: 'card-edge'; cardId: string; edge: SnapEdge }
  | { type: 'merge'; cardId: string }
  | { type: 'page-edge'; edge: SnapEdge }
  | { type: 'sidebar-dock'; edge: SidebarDock }
  | { type: 'canvas'; el: Element };

interface ActiveDrag {
  spec: DockDragSpec;
  startX: number;
  startY: number;
  lastX: number;
  lastY: number;
  active: boolean;
  hover: Hover | null;
}

/// 触发拖拽所需的最小移动距离 (px) — 与 HTML5 DnD 阈值相当
const THRESHOLD = 5;
/// 释放动画时长 (ms) — 与 DockDragGhost 的 transition 时长一致
const RELEASE_MS = 180;

let drag: ActiveDrag | null = null;
/// 最近一次拖拽是否激活 — 用于抑制激活拖拽后跟随的 click (否则拖完会误触发点击)
let suppressNextClick = false;
/// 释放动画代际 — 新拖拽开始时递增, 使挂起的清除定时器失效
let ghostEpoch = 0;

/// 低动画偏好 (无障碍) — matchMedia 不可用 (如 jsdom) 时视为低动画, 不放释放动画
function prefersReducedMotion(): boolean {
  return (
    typeof window.matchMedia !== 'function' ||
    window.matchMedia('(prefers-reduced-motion: reduce)').matches
  );
}
/// 画布投放处理 — 按画布元素注册 (多个控制卡片并存时, 落点归属指针下方的画布)
type CanvasHandler = (x: number, y: number, spec: WidgetDragSpec) => void;
const canvasHandlers = new Map<Element, CanvasHandler>();
let canvasHover = false;

const ghostSubs = new Set<(g: GhostState | null) => void>();
const canvasHoverSubs = new Set<(h: boolean) => void>();

function emitGhost(g: GhostState | null) {
  ghostSubs.forEach((fn) => fn(g));
}

function emitCanvasHover(h: boolean) {
  if (h === canvasHover) return;
  canvasHover = h;
  canvasHoverSubs.forEach((fn) => fn(h));
}

/** 拖拽源 pointerdown 入口。不 preventDefault — 点击/双击/右键菜单照常工作。 */
export function begin(
  e: { clientX: number; clientY: number; button?: number; pointerId?: number; target?: EventTarget | null },
  spec: DockDragSpec
): void {
  if (e.button !== undefined && e.button !== 0) return;
  if (drag) return; // 防御: 一次只允许一个拖拽
  suppressNextClick = false;
  ghostEpoch++; // 使上一次释放动画的清除定时器失效
  // 指针捕获: 在窗口外释放时 pointerup 仍会投递到捕获目标, 避免拖拽卡死
  const target = e.target as Element | null;
  if (target && typeof target.setPointerCapture === 'function' && e.pointerId !== undefined) {
    try {
      target.setPointerCapture(e.pointerId);
    } catch {
      /* 指针已失效时忽略 */
    }
  }
  drag = { spec, startX: e.clientX, startY: e.clientY, lastX: e.clientX, lastY: e.clientY, active: false, hover: null };
  window.addEventListener('pointermove', onMove);
  window.addEventListener('pointerup', onUp);
  window.addEventListener('pointercancel', onCancel);
  window.addEventListener('keydown', onKeyDown);
  window.addEventListener('blur', onCancel);
}

function onMove(e: PointerEvent) {
  const d = drag;
  if (!d) return;
  d.lastX = e.clientX;
  d.lastY = e.clientY;
  if (!d.active) {
    if (Math.hypot(e.clientX - d.startX, e.clientY - d.startY) < THRESHOLD) return;
    d.active = true;
    activate(d.spec);
  }
  emitGhost({ x: d.lastX, y: d.lastY, label: d.spec.label });
  // store 各 setter 均有同值去重, 高频调用无副作用
  d.hover = hitTest(e.clientX, e.clientY);
  applyHover(d.hover);
}

function activate(spec: DockDragSpec) {
  const st = useDockStore.getState();
  if (spec.kind === 'tab') st.setDraggingTab(spec.tab);
  else if (spec.kind === 'card') st.setDraggingCard(spec.cardId);
  else if (spec.kind === 'sidebar') useLayoutStore.getState().setDraggingSidebar(true);
  // 拖拽期间全局禁止文字选择 — 防止经过 select-text 区域 (如 RawData 数值视图) 时误选
  document.body.classList.add('dragging-dock');
}

function onUp(e: PointerEvent) {
  if (e.button !== 0) return;
  const d = drag;
  if (!d) return;
  drag = null;
  cleanupListeners();
  document.body.classList.remove('dragging-dock');
  if (d.active) {
    suppressNextClick = true;
    commit(d);
  }
  finish();
  if (d.active && !prefersReducedMotion()) {
    // 释放动画: 幽灵在落点放大淡出, 动画结束后清除 (代际校验防止误清新拖拽的幽灵)
    const epoch = ghostEpoch;
    emitGhost({ x: d.lastX, y: d.lastY, label: d.spec.label, releasing: true });
    setTimeout(() => {
      if (epoch === ghostEpoch) emitGhost(null);
    }, RELEASE_MS);
  } else {
    emitGhost(null);
  }
  emitCanvasHover(false);
}

function onCancel() {
  const d = drag;
  if (!d) return;
  drag = null;
  cleanupListeners();
  document.body.classList.remove('dragging-dock');
  finish();
  emitGhost(null);
  emitCanvasHover(false);
}

function onKeyDown(e: KeyboardEvent) {
  if (e.key === 'Escape') onCancel();
}

function cleanupListeners() {
  window.removeEventListener('pointermove', onMove);
  window.removeEventListener('pointerup', onUp);
  window.removeEventListener('pointercancel', onCancel);
  window.removeEventListener('keydown', onKeyDown);
  window.removeEventListener('blur', onCancel);
}

/// 提交落点 — 复用 store 既有动作 (内部会清理 dragging* 状态)
function commit(d: ActiveDrag) {
  const h = d.hover;
  if (!h) return;
  const st = useDockStore.getState();
  switch (h.type) {
    case 'card-edge':
      st.dropOnCardEdge(h.cardId, h.edge);
      break;
    case 'merge':
      st.moveTabToCard(h.cardId);
      break;
    case 'page-edge':
      st.dropOnRootEdge(h.edge);
      break;
    case 'sidebar-dock':
      useLayoutStore.getState().setSidebarDock(h.edge);
      break;
    case 'canvas': {
      const s = d.spec;
      if (s.kind === 'widget') {
        const fn = canvasHandlers.get(h.el);
        if (fn) fn(d.lastX, d.lastY, s.widget);
      }
      break;
    }
  }
}

/// 收尾: 清理未被子动作消费的拖拽/悬停状态
function finish() {
  const st = useDockStore.getState();
  if (st.draggingTab) st.setDraggingTab(null);
  if (st.draggingCardId) st.setDraggingCard(null);
  if (st.mergeHoverCardId) st.setMergeHover(null);
  if (st.dropTarget) st.setDropTarget(null);
  const ls = useLayoutStore.getState();
  if (ls.draggingSidebar) ls.setDraggingSidebar(false);
  if (ls.dockEdgeHover) ls.setDockEdgeHover(null);
}

/// 命中测试: 找指针下最近的投放区。控件拖拽会命中 canvas, 其余拖拽穿透到卡片/页面区;
/// merge 区对当前拖拽无效时 (如整卡拖拽、不同 kind) 穿透到外层的 card-edge 区。
function hitTest(x: number, y: number): Hover | null {
  const el = document.elementFromPoint(x, y);
  if (!el) return null;
  let zone: Element | null = el.closest('[data-dock-zone]');
  while (zone) {
    const type = zone.getAttribute('data-dock-zone');
    let skip = type === 'canvas' && (!drag || drag.spec.kind !== 'widget');
    if (type === 'merge') {
      const cardId = zone.getAttribute('data-dock-card');
      const kind = zone.getAttribute('data-dock-kind') as CardKind | null;
      const st = useDockStore.getState();
      skip = !(
        cardId &&
        st.draggingTab &&
        kind === st.draggingTab.kind &&
        st.draggingTab.fromCardId !== cardId
      );
    }
    if (skip) {
      zone = zone.parentElement ? zone.parentElement.closest('[data-dock-zone]') : null;
      continue;
    }
    break;
  }
  if (!zone) return null;

  const type = zone.getAttribute('data-dock-zone')!;
  const edge = zone.getAttribute('data-dock-edge') as SnapEdge | null;
  const cardId = zone.getAttribute('data-dock-card');
  const kind = zone.getAttribute('data-dock-kind') as CardKind | null;
  const st = useDockStore.getState();

  switch (type) {
    case 'card-edge': {
      if (!cardId) return null;
      const card = st.cards[cardId];
      if (!card) return null;
      // 同卡多 Tab 可拆自身; 整卡拖拽不能落回自身
      const valid = st.draggingTab
        ? st.draggingTab.fromCardId !== cardId || card.tabIds.length > 1
        : st.draggingCardId
          ? st.draggingCardId !== cardId
          : false;
      if (!valid) return null;
      const r = zone.getBoundingClientRect();
      if (r.width <= 0 || r.height <= 0) return null;
      const rx = (x - r.left) / r.width;
      const ry = (y - r.top) / r.height;
      const resolved: SnapEdge = rx < 0.3 ? 'left' : rx > 0.7 ? 'right' : ry < 0.5 ? 'top' : 'bottom';
      return { type: 'card-edge', cardId, edge: resolved };
    }
    case 'merge': {
      if (!cardId || !st.draggingTab) return null;
      const card = st.cards[cardId];
      if (!card || kind !== card.kind) return null;
      if (st.draggingTab.kind !== card.kind || st.draggingTab.fromCardId === cardId) return null;
      return { type: 'merge', cardId };
    }
    case 'page-edge': {
      if (!st.draggingTab && !st.draggingCardId) return null;
      if (!edge) return null;
      return { type: 'page-edge', edge };
    }
    case 'sidebar-dock': {
      if (!useLayoutStore.getState().draggingSidebar || !edge) return null;
      return { type: 'sidebar-dock', edge: edge as SidebarDock };
    }
    case 'canvas':
      return { type: 'canvas', el: zone };
    default:
      return null;
  }
}

/// 应用悬停状态 → store 预览 (dropTarget 驱动 DockLayout 全局预览)
function applyHover(h: Hover | null) {
  const st = useDockStore.getState();
  const ls = useLayoutStore.getState();
  if (!h) {
    if (st.dropTarget) st.setDropTarget(null);
    if (st.mergeHoverCardId) st.setMergeHover(null);
    if (ls.dockEdgeHover) ls.setDockEdgeHover(null);
    emitCanvasHover(false);
    return;
  }
  switch (h.type) {
    case 'card-edge':
      st.setDropTarget({ cardId: h.cardId, edge: h.edge });
      if (st.mergeHoverCardId) st.setMergeHover(null);
      emitCanvasHover(false);
      break;
    case 'page-edge':
      st.setDropTarget({ cardId: null, edge: h.edge });
      if (st.mergeHoverCardId) st.setMergeHover(null);
      emitCanvasHover(false);
      break;
    case 'merge':
      if (st.dropTarget) st.setDropTarget(null);
      st.setMergeHover(h.cardId);
      emitCanvasHover(false);
      break;
    case 'sidebar-dock':
      ls.setDockEdgeHover(h.edge);
      break;
    case 'canvas':
      emitCanvasHover(true);
      break;
  }
}

/** 激活拖拽后消费一次 click — 返回 true 表示该 click 应被忽略 (拖完误触发点击) */
export function consumeClick(): boolean {
  const v = suppressNextClick;
  suppressNextClick = false;
  return v;
}

/** 幽灵元素订阅 — 返回取消订阅函数 */
export function subscribeGhost(fn: (g: GhostState | null) => void): () => void {
  ghostSubs.add(fn);
  fn(drag && drag.active ? { x: drag.lastX, y: drag.lastY, label: drag.spec.label } : null);
  return () => {
    ghostSubs.delete(fn);
  };
}

/** 画布悬停订阅 (控件拖拽时的画布高亮) */
export function subscribeCanvasHover(fn: (h: boolean) => void): () => void {
  canvasHoverSubs.add(fn);
  fn(canvasHover);
  return () => {
    canvasHoverSubs.delete(fn);
  };
}

/** 注册画布元素的投放处理 (NodeEditor 挂载时注册, 卸载时注销) */
export function registerCanvasHandler(el: Element | null, fn: CanvasHandler | null): void {
  if (!el) return;
  if (fn) canvasHandlers.set(el, fn);
  else canvasHandlers.delete(el);
}

export function isDragging(): boolean {
  return drag?.active ?? false;
}

/// 组件侧统一入口
export const dockDrag = {
  begin,
  consumeClick,
  subscribeGhost,
  subscribeCanvasHover,
  registerCanvasHandler,
  isDragging,
};

/** 仅供测试使用 — 重置模块级状态 */
export function __resetForTests(): void {
  if (drag) {
    drag = null;
    cleanupListeners();
    document.body.classList.remove('dragging-dock');
  }
  suppressNextClick = false;
  ghostEpoch++; // 使挂起的释放动画定时器失效
  canvasHandlers.clear();
  emitCanvasHover(false);
  emitGhost(null);
  canvasHoverSubs.clear();
  ghostSubs.clear();
}
