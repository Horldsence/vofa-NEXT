import { beforeEach, describe, expect, it, vi } from 'vitest';

// dockStore/layoutStore 的 persist 中间件需要 localStorage — 在导入 store 前提供内存桩
vi.hoisted(() => {
  const store = new Map<string, string>();
  const localStorageMock = {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => void store.set(key, value),
    removeItem: (key: string) => void store.delete(key),
    clear: () => store.clear(),
    key: (index: number) => [...store.keys()][index] ?? null,
    get length() {
      return store.size;
    },
  };
  const g = globalThis as { localStorage?: unknown };
  g.localStorage = localStorageMock;
});

import { dockDrag, __resetForTests, type WidgetDragSpec } from '../dockDrag';
import { useDockStore } from '../../store/dockStore';
import { useLayoutStore } from '../../store/layoutStore';

/// 构造投放区元素: 挂在 document.body 上, 供 elementFromPoint 命中
function makeZone(attrs: Record<string, string>, parent?: HTMLElement): HTMLElement {
  const el = document.createElement('div');
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v);
  (parent ?? document.body).appendChild(el);
  return el;
}

function stubRect(el: HTMLElement, rect: Partial<DOMRect>) {
  el.getBoundingClientRect = () =>
    ({
      left: 0,
      top: 0,
      right: 0,
      bottom: 0,
      width: 0,
      height: 0,
      x: 0,
      y: 0,
      toJSON: () => ({}),
      ...rect,
    }) as DOMRect;
}

function moveTo(x: number, y: number) {
  window.dispatchEvent(new MouseEvent('pointermove', { clientX: x, clientY: y, button: 0 }));
}

function releaseAt(x: number, y: number) {
  window.dispatchEvent(new MouseEvent('pointerup', { clientX: x, clientY: y, button: 0 }));
}

const INITIAL_ROOT = useDockStore.getState().root;
const INITIAL_CARDS = useDockStore.getState().cards;

function seedCards() {
  useDockStore.setState({
    root: {
      id: 'split-root',
      type: 'split',
      dir: 'row',
      children: [
        { id: 'n1', type: 'card', cardId: 'control-main' },
        { id: 'n2', type: 'card', cardId: 'control-2' },
        { id: 'n3', type: 'card', cardId: 'data-main' },
      ],
      sizes: [40, 30, 30],
    },
    cards: {
      'control-main': { id: 'control-main', kind: 'control', tabIds: ['t1', 't2'], activeTabId: 't1' },
      'control-2': { id: 'control-2', kind: 'control', tabIds: ['t3'], activeTabId: 't3' },
      'data-main': { id: 'data-main', kind: 'data', tabIds: ['d1'], activeTabId: 'd1' },
    },
  });
}

describe('dockDrag 指针拖拽控制器', () => {
  beforeEach(() => {
    __resetForTests();
    useDockStore.setState({ root: INITIAL_ROOT, cards: INITIAL_CARDS });
    useLayoutStore.setState({
      sidebarDock: 'left',
      draggingSidebar: false,
      dockEdgeHover: null,
      aiPanelVisible: false,
      aiDock: 'right',
      aiFloatRect: { x: 220, y: 120, w: 400, h: 480 },
      draggingAiPanel: false,
      aiDockEdgeHover: null,
    });
    document.body.innerHTML = '';
    // jsdom 未实现 elementFromPoint — 默认返回 null (无投放区), 需要时用 spyOn 覆盖
    document.elementFromPoint = () => null;
  });

  it('非左键不启动拖拽', () => {
    dockDrag.begin({ clientX: 10, clientY: 10, button: 2 }, { kind: 'sidebar', label: 'Sidebar' });
    moveTo(100, 100);
    expect(useLayoutStore.getState().draggingSidebar).toBe(false);
  });

  it('移动未超过阈值不激活, 超过阈值后激活并写入 store 状态', () => {
    dockDrag.begin({ clientX: 10, clientY: 10, button: 0 }, { kind: 'sidebar', label: 'Sidebar' });
    moveTo(12, 12); // < 5px
    expect(useLayoutStore.getState().draggingSidebar).toBe(false);

    moveTo(40, 40); // > 5px
    expect(useLayoutStore.getState().draggingSidebar).toBe(true);
    expect(document.body.classList.contains('dragging-dock')).toBe(true);
  });

  it('激活的拖拽提交到卡片边缘 (card-edge) — 拆分落点生效', () => {
    seedCards();
    const zone = makeZone({
      'data-dock-zone': 'card-edge',
      'data-dock-card': 'control-2',
      'data-dock-kind': 'control',
    });
    stubRect(zone, { left: 0, top: 0, width: 200, height: 100 });

    const from = useDockStore.getState().cards['control-main'];
    dockDrag.begin(
      { clientX: 10, clientY: 10, button: 0 },
      { kind: 'tab', tab: { kind: 'control', tabId: 't1', fromCardId: from.id }, label: 'Tab A' }
    );
    moveTo(40, 40); // 激活
    expect(useDockStore.getState().draggingTab).not.toBeNull();

    // 悬停在卡片右半区 → right 边缘
    vi.spyOn(document, 'elementFromPoint').mockReturnValue(zone);
    moveTo(280, 50);
    expect(useDockStore.getState().dropTarget).toEqual({ cardId: 'control-2', edge: 'right' });

    const rootBefore = useDockStore.getState().root;
    releaseAt(280, 50);
    expect(useDockStore.getState().draggingTab).toBeNull();
    expect(useDockStore.getState().dropTarget).toBeNull();
    // 落点生效: 树发生拆分变化
    expect(useDockStore.getState().root).not.toBe(rootBefore);
    vi.restoreAllMocks();
  });

  it('同 kind 跨卡片标题栏悬停为合并目标, 释放后 moveTabToCard', () => {
    seedCards();
    const target = makeZone({
      'data-dock-zone': 'merge',
      'data-dock-card': 'control-2',
      'data-dock-kind': 'control',
    });
    stubRect(target, { left: 0, top: 0, width: 200, height: 100 });

    const from = useDockStore.getState().cards['control-main'];
    dockDrag.begin(
      { clientX: 10, clientY: 10, button: 0 },
      { kind: 'tab', tab: { kind: 'control', tabId: 't2', fromCardId: from.id }, label: 'Tab B' }
    );
    moveTo(40, 40); // 激活

    vi.spyOn(document, 'elementFromPoint').mockReturnValue(target);
    moveTo(100, 50);
    expect(useDockStore.getState().mergeHoverCardId).toBe('control-2');
    expect(useDockStore.getState().dropTarget).toBeNull();

    releaseAt(100, 50);
    const targetCard = useDockStore.getState().cards['control-2'];
    expect(targetCard.tabIds).toContain('t2');
    expect(useDockStore.getState().mergeHoverCardId).toBeNull();
    expect(useDockStore.getState().draggingTab).toBeNull();
    vi.restoreAllMocks();
  });

  it('不同 kind 的卡片标题栏不是合并目标', () => {
    seedCards();
    const target = makeZone({
      'data-dock-zone': 'merge',
      'data-dock-card': 'data-main',
      'data-dock-kind': 'data',
    });
    stubRect(target, { left: 0, top: 0, width: 200, height: 100 });

    const from = useDockStore.getState().cards['control-main'];
    dockDrag.begin(
      { clientX: 10, clientY: 10, button: 0 },
      { kind: 'tab', tab: { kind: 'control', tabId: 't1', fromCardId: from.id }, label: 'Tab A' }
    );
    moveTo(40, 40);

    vi.spyOn(document, 'elementFromPoint').mockReturnValue(target);
    moveTo(100, 50);
    expect(useDockStore.getState().mergeHoverCardId).toBeNull();
    expect(useDockStore.getState().dropTarget).toBeNull();
    vi.restoreAllMocks();
  });

  it('页面边缘热区 (page-edge) 提交到 dropOnRootEdge', () => {
    seedCards();
    const zone = makeZone({ 'data-dock-zone': 'page-edge', 'data-dock-edge': 'left' });

    const from = useDockStore.getState().cards['control-main'];
    dockDrag.begin(
      { clientX: 10, clientY: 10, button: 0 },
      { kind: 'tab', tab: { kind: 'control', tabId: 't1', fromCardId: from.id }, label: 'Tab A' }
    );
    moveTo(40, 40);

    vi.spyOn(document, 'elementFromPoint').mockReturnValue(zone);
    moveTo(100, 100);
    expect(useDockStore.getState().dropTarget).toEqual({ cardId: null, edge: 'left' });

    const rootBefore = useDockStore.getState().root;
    releaseAt(100, 100);
    expect(useDockStore.getState().root).not.toBe(rootBefore);
    expect(useDockStore.getState().dropTarget).toBeNull();
    vi.restoreAllMocks();
  });

  it('侧边栏拖拽悬停窗口边缘 → 预览 + 释放后切换停靠侧', () => {
    const zone = makeZone({ 'data-dock-zone': 'sidebar-dock', 'data-dock-edge': 'right' });

    dockDrag.begin({ clientX: 10, clientY: 10, button: 0 }, { kind: 'sidebar', label: 'Sidebar' });
    moveTo(40, 40);
    expect(useLayoutStore.getState().draggingSidebar).toBe(true);

    vi.spyOn(document, 'elementFromPoint').mockReturnValue(zone);
    moveTo(100, 100);
    expect(useLayoutStore.getState().dockEdgeHover).toBe('right');

    releaseAt(100, 100);
    expect(useLayoutStore.getState().sidebarDock).toBe('right');
    expect(useLayoutStore.getState().draggingSidebar).toBe(false);
    expect(useLayoutStore.getState().dockEdgeHover).toBeNull();
    vi.restoreAllMocks();
  });

  it('AI 面板拖拽: 悬停边缘热区 → 预览 + 释放后停靠对应边', () => {
    const zone = makeZone({ 'data-dock-zone': 'ai-dock', 'data-dock-edge': 'bottom' });

    dockDrag.begin({ clientX: 10, clientY: 10, button: 0 }, { kind: 'ai-panel', label: 'AI' });
    moveTo(40, 40);
    expect(useLayoutStore.getState().draggingAiPanel).toBe(true);

    vi.spyOn(document, 'elementFromPoint').mockReturnValue(zone);
    moveTo(100, 100);
    expect(useLayoutStore.getState().aiDockEdgeHover).toBe('bottom');

    releaseAt(100, 100);
    expect(useLayoutStore.getState().aiDock).toBe('bottom');
    expect(useLayoutStore.getState().draggingAiPanel).toBe(false);
    expect(useLayoutStore.getState().aiDockEdgeHover).toBeNull();
    vi.restoreAllMocks();
  });

  it('AI 面板拖拽: 空白处释放 → 浮动且落点成为浮窗位置 (clamp 窗口内)', () => {
    dockDrag.begin({ clientX: 10, clientY: 10, button: 0 }, { kind: 'ai-panel', label: 'AI' });
    moveTo(400, 300);
    releaseAt(400, 300);

    const st = useLayoutStore.getState();
    expect(st.aiDock).toBe('float');
    expect(st.aiFloatRect.x).toBeGreaterThanOrEqual(8);
    expect(st.aiFloatRect.y).toBeGreaterThanOrEqual(8);
    expect(st.aiFloatRect.w).toBe(400);
    expect(st.draggingAiPanel).toBe(false);
  });

  it('未悬停有效投放区时释放 → 取消拖拽, 不产生落点', () => {
    seedCards();
    dockDrag.begin(
      { clientX: 10, clientY: 10, button: 0 },
      { kind: 'card', cardId: 'control-main', label: 'Card' }
    );
    moveTo(40, 40);
    expect(useDockStore.getState().draggingCardId).toBe('control-main');

    vi.spyOn(document, 'elementFromPoint').mockReturnValue(null);
    moveTo(200, 200);
    expect(useDockStore.getState().dropTarget).toBeNull();

    const rootBefore = useDockStore.getState().root;
    releaseAt(200, 200);
    expect(useDockStore.getState().draggingCardId).toBeNull();
    expect(useDockStore.getState().root).toBe(rootBefore);
    vi.restoreAllMocks();
  });

  it('Escape 取消拖拽并清理状态', () => {
    seedCards();
    dockDrag.begin(
      { clientX: 10, clientY: 10, button: 0 },
      { kind: 'card', cardId: 'control-main', label: 'Card' }
    );
    moveTo(40, 40);
    expect(useDockStore.getState().draggingCardId).toBe('control-main');

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(useDockStore.getState().draggingCardId).toBeNull();
    expect(document.body.classList.contains('dragging-dock')).toBe(false);
  });

  it('激活拖拽后 consumeClick 返回 true (抑制拖完的误点击), 普通点击返回 false', () => {
    // 普通点击: 未激活
    dockDrag.begin({ clientX: 10, clientY: 10, button: 0 }, { kind: 'sidebar', label: 'Sidebar' });
    releaseAt(12, 12);
    expect(dockDrag.consumeClick()).toBe(false);

    // 激活拖拽后释放 → 下一次 click 被抑制
    dockDrag.begin({ clientX: 10, clientY: 10, button: 0 }, { kind: 'sidebar', label: 'Sidebar' });
    moveTo(40, 40);
    releaseAt(40, 40);
    expect(dockDrag.consumeClick()).toBe(true);
    expect(dockDrag.consumeClick()).toBe(false);
  });

  it('控件拖拽悬停画布 → 高亮; 释放 → 调用对应画布元素的投放处理并携带 spec', () => {
    seedCards();
    const zone = makeZone({ 'data-dock-zone': 'canvas' });
    stubRect(zone, { left: 0, top: 0, width: 400, height: 300 });

    const hoverSpy = vi.fn();
    dockDrag.subscribeCanvasHover(hoverSpy);

    const handler = vi.fn();
    dockDrag.registerCanvasHandler(zone, (x, y, spec) => handler(x, y, spec));

    const spec: WidgetDragSpec = { kind: 'Math', op: 'add' };
    dockDrag.begin({ clientX: 10, clientY: 10, button: 0 }, { kind: 'widget', widget: spec, label: 'Math add' });
    moveTo(40, 40);

    vi.spyOn(document, 'elementFromPoint').mockReturnValue(zone);
    moveTo(120, 80);
    expect(hoverSpy).toHaveBeenLastCalledWith(true);

    releaseAt(120, 80);
    expect(handler).toHaveBeenCalledWith(120, 80, spec);
    expect(hoverSpy).toHaveBeenLastCalledWith(false);
    vi.restoreAllMocks();
  });

  it('控件落点归属指针下方画布元素注册的处理, 而非最后注册者', () => {
    seedCards();
    const zoneA = makeZone({ 'data-dock-zone': 'canvas' });
    const zoneB = makeZone({ 'data-dock-zone': 'canvas' });
    stubRect(zoneA, { left: 0, top: 0, width: 400, height: 300 });
    stubRect(zoneB, { left: 0, top: 0, width: 400, height: 300 });

    const handlerA = vi.fn();
    const handlerB = vi.fn();
    dockDrag.registerCanvasHandler(zoneA, (x, y, spec) => handlerA(x, y, spec));
    dockDrag.registerCanvasHandler(zoneB, (x, y, spec) => handlerB(x, y, spec));

    const spec: WidgetDragSpec = { kind: 'Knob' };
    dockDrag.begin({ clientX: 10, clientY: 10, button: 0 }, { kind: 'widget', widget: spec, label: 'Knob' });
    moveTo(40, 40);

    vi.spyOn(document, 'elementFromPoint').mockReturnValue(zoneA);
    moveTo(120, 80);
    releaseAt(120, 80);
    expect(handlerA).toHaveBeenCalledWith(120, 80, spec);
    expect(handlerB).not.toHaveBeenCalled();
    vi.restoreAllMocks();
  });

  it('整卡拖拽悬停他人标题栏 (merge 区) 时穿透到 card-edge 投放区', () => {
    seedCards();
    const cardZone = makeZone({
      'data-dock-zone': 'card-edge',
      'data-dock-card': 'control-2',
      'data-dock-kind': 'control',
    });
    stubRect(cardZone, { left: 0, top: 0, width: 200, height: 100 });
    const titleBar = makeZone(
      { 'data-dock-zone': 'merge', 'data-dock-card': 'control-2', 'data-dock-kind': 'control' },
      cardZone
    );
    stubRect(titleBar, { left: 0, top: 0, width: 200, height: 28 });

    dockDrag.begin(
      { clientX: 10, clientY: 10, button: 0 },
      { kind: 'card', cardId: 'control-main', label: 'Card' }
    );
    moveTo(40, 40);
    expect(useDockStore.getState().draggingCardId).toBe('control-main');

    // 指针在标题栏上 → merge 无效 (整卡拖拽) → 穿透到 card-edge
    vi.spyOn(document, 'elementFromPoint').mockReturnValue(titleBar);
    moveTo(280, 15);
    expect(useDockStore.getState().dropTarget).toEqual({ cardId: 'control-2', edge: 'right' });
    vi.restoreAllMocks();
  });

  it('非控件拖拽穿透画布区, 命中外层卡片的 card-edge 投放区', () => {
    seedCards();
    const cardZone = makeZone({
      'data-dock-zone': 'card-edge',
      'data-dock-card': 'control-2',
      'data-dock-kind': 'control',
    });
    stubRect(cardZone, { left: 0, top: 0, width: 200, height: 100 });
    const canvasZone = makeZone({ 'data-dock-zone': 'canvas' }, cardZone);
    stubRect(canvasZone, { left: 0, top: 0, width: 200, height: 100 });

    const from = useDockStore.getState().cards['control-main'];
    dockDrag.begin(
      { clientX: 10, clientY: 10, button: 0 },
      { kind: 'tab', tab: { kind: 'control', tabId: 't1', fromCardId: from.id }, label: 'Tab A' }
    );
    moveTo(40, 40);

    vi.spyOn(document, 'elementFromPoint').mockReturnValue(canvasZone);
    moveTo(280, 50);
    // 穿透 canvas → 命中 card-edge → right
    expect(useDockStore.getState().dropTarget).toEqual({ cardId: 'control-2', edge: 'right' });
    vi.restoreAllMocks();
  });
});
