import { create } from 'zustand';
import { persist, createJSONStorage } from 'zustand/middleware';

/// 侧边栏停靠侧
export type SidebarDock = 'left' | 'right';

interface LayoutState {
  sidebarDock: SidebarDock;
  /// 正在拖拽侧边栏标题栏 (不持久化) — 用于窗口左右边缘停靠区高亮
  draggingSidebar: boolean;
  setSidebarDock: (d: SidebarDock) => void;
  setDraggingSidebar: (dragging: boolean) => void;
}

/// 侧边栏布局 store — 中央区的模块编排由 dockStore 负责
export const useLayoutStore = create<LayoutState>()(
  persist(
    (set) => ({
      sidebarDock: 'left',
      draggingSidebar: false,
      setSidebarDock: (sidebarDock) => set({ sidebarDock }),
      setDraggingSidebar: (draggingSidebar) => set({ draggingSidebar }),
    }),
    {
      name: 'vofa-layout',
      storage: createJSONStorage(() => localStorage),
      partialize: (s) => ({ sidebarDock: s.sidebarDock }),
    }
  )
);
