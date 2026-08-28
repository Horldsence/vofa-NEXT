import { create } from 'zustand';
import { persist, createJSONStorage } from 'zustand/middleware';

/// 侧边栏停靠侧
export type SidebarDock = 'left' | 'right';

/// AI 面板停靠位置 — 右/左/下停靠或浮动小窗
export type AiPanelDock = 'right' | 'left' | 'bottom' | 'float';
/// AI 面板可停靠的窗口边缘
export type AiDockEdge = Exclude<AiPanelDock, 'float'>;

/// 浮动窗口矩形 (相对窗口客户区, px)
export interface AiFloatRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/// 浮动窗口最小尺寸
export const AI_FLOAT_MIN_W = 320;
export const AI_FLOAT_MIN_H = 260;
const AI_FLOAT_DEFAULT: AiFloatRect = { x: 220, y: 120, w: 400, h: 480 };

interface LayoutState {
  sidebarDock: SidebarDock;
  /// 正在拖拽侧边栏标题栏 (不持久化) — 用于窗口左右边缘停靠区高亮
  draggingSidebar: boolean;
  /// 侧边栏拖拽时指针悬停的窗口边缘 (不持久化) — 停靠预览
  dockEdgeHover: SidebarDock | null;
  /// AI 面板可见性
  aiPanelVisible: boolean;
  /// AI 面板停靠位置
  aiDock: AiPanelDock;
  /// 浮动窗口矩形
  aiFloatRect: AiFloatRect;
  /// 正在拖拽 AI 面板标题栏 (不持久化) — 窗口边缘热区 + 浮动投放
  draggingAiPanel: boolean;
  /// AI 面板拖拽悬停的停靠边缘 (不持久化)
  aiDockEdgeHover: AiDockEdge | null;
  setSidebarDock: (d: SidebarDock) => void;
  setDraggingSidebar: (dragging: boolean) => void;
  setDockEdgeHover: (d: SidebarDock | null) => void;
  setAiPanelVisible: (v: boolean) => void;
  setAiDock: (d: AiPanelDock) => void;
  setAiFloatRect: (r: AiFloatRect) => void;
  /// 拖拽松手落为浮动 — 以指针落点为标题栏位置放置 (clamp 到窗口内)
  dropAiToFloat: (x: number, y: number) => void;
  setDraggingAiPanel: (dragging: boolean) => void;
  setAiDockEdgeHover: (e: AiDockEdge | null) => void;
}

/// 侧边栏与 AI 面板布局 store — 中央区的模块编排由 dockStore 负责
export const useLayoutStore = create<LayoutState>()(
  persist(
    (set, get) => ({
      sidebarDock: 'left',
      draggingSidebar: false,
      dockEdgeHover: null,
      aiPanelVisible: false,
      aiDock: 'right',
      aiFloatRect: AI_FLOAT_DEFAULT,
      draggingAiPanel: false,
      aiDockEdgeHover: null,
      setSidebarDock: (sidebarDock) => set({ sidebarDock }),
      setDraggingSidebar: (draggingSidebar) => set({ draggingSidebar }),
      setDockEdgeHover: (dockEdgeHover) =>
        set((state) => (state.dockEdgeHover === dockEdgeHover ? state : { dockEdgeHover })),
      setAiPanelVisible: (aiPanelVisible) => set({ aiPanelVisible }),
      setAiDock: (aiDock) => set({ aiDock }),
      setAiFloatRect: (aiFloatRect) => set({ aiFloatRect }),
      dropAiToFloat: (x, y) => {
        const rect = get().aiFloatRect;
        const maxX = Math.max(8, window.innerWidth - rect.w - 8);
        const maxY = Math.max(8, window.innerHeight - rect.h - 8);
        set({
          aiDock: 'float',
          aiFloatRect: {
            ...rect,
            x: Math.min(Math.max(x - rect.w / 2, 8), maxX),
            y: Math.min(Math.max(y - 14, 8), maxY),
          },
        });
      },
      setDraggingAiPanel: (draggingAiPanel) => set({ draggingAiPanel }),
      setAiDockEdgeHover: (aiDockEdgeHover) =>
        set((state) => (state.aiDockEdgeHover === aiDockEdgeHover ? state : { aiDockEdgeHover })),
    }),
    {
      name: 'vofa-layout',
      storage: createJSONStorage(() => localStorage),
      partialize: (s) => ({
        sidebarDock: s.sidebarDock,
        aiPanelVisible: s.aiPanelVisible,
        aiDock: s.aiDock,
        aiFloatRect: s.aiFloatRect,
      }),
    }
  )
);
