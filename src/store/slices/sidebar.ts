import type { Lang } from '../../i18n';

export type SidebarView = 'transport' | 'protocol' | 'widgets';

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

    sidebarView: 'transport',
    sidebarVisible: true,
    setSidebarView: (view) => set({ sidebarView: view, sidebarVisible: true }),
    toggleSidebar: (view) => {
      const { sidebarView, sidebarVisible } = get();
      if (sidebarView === view && sidebarVisible) {
        set({ sidebarVisible: false });
      } else {
        set({ sidebarView: view, sidebarVisible: true });
      }
    },
  };
}
