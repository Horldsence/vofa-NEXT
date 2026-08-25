import type { Lang } from '../../i18n';

export type SidebarView = 'widgets' | 'quickstart' | 'panels';

export interface SidebarSlice {
  lang: Lang;
  setLang: (lang: Lang) => void;
  sidebarView: SidebarView;
  sidebarVisible: boolean;
  setSidebarView: (view: SidebarView) => void;
  toggleSidebar: (view: SidebarView) => void;
}

export function createSidebarSlice(set: any, get: any): SidebarSlice {
  return {
    lang: 'zh',
    setLang: (lang) => set({ lang }),

    sidebarView: 'widgets',
    sidebarVisible: true,
    setSidebarView: (view) => set({ sidebarView: view, sidebarVisible: true }),
    toggleSidebar: (view) => {
      const { sidebarView, sidebarVisible } = get();
      // 若点击的 view 与当前相同且可见 → 收起; 否则切换到该 view 并展开。
      // 「panels」 与其他 view 同等对待, 不影响原逻辑。
      if (sidebarView === view && sidebarVisible) {
        set({ sidebarVisible: false });
      } else {
        set({ sidebarView: view, sidebarVisible: true });
      }
    },
  };
}
