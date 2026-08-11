import { useEffect, useMemo, useState } from 'react';
import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels';
import { listen } from '@tauri-apps/api/event';
import { Settings, Info, RefreshCw, PanelLeft } from 'lucide-react';
import { ActivityBar } from './components/layout/ActivityBar';
import { Sidebar } from './components/layout/Sidebar';
import { DockLayout } from './components/layout/DockLayout';
import { StatusBar } from './components/layout/StatusBar';
import { NotificationToasts } from './components/NotificationToasts';
import { SettingsModal } from './components/SettingsModal';
import { AboutModal } from './components/AboutModal';
import { CustomWidgetEditorContainer } from './components/CustomWidgetEditorContainer';
import { OnboardingWizard } from './components/onboarding/OnboardingWizard';
import { HelpCenterModal } from './components/onboarding/HelpCenterModal';
import { ContextMenu } from './components/ui/ContextMenu';
import { useContextMenu } from './lib/hooks/useContextMenu';
import { useAppStore } from './store/appStore';
import { useSettingsStore } from './store/settingsStore';
import { useOnboardingStore } from './store/onboardingStore';
import { useLayoutStore } from './store/layoutStore';
import { t } from './i18n';
import { createWidget } from './lib/utils/createWidget';

function App() {
  const initEventListeners = useAppStore((s) => s.initEventListeners);
  const refreshPorts = useAppStore((s) => s.refreshPorts);
  const sidebarView = useAppStore((s) => s.sidebarView);
  const sidebarVisible = useAppStore((s) => s.sidebarVisible);
  const toggleSidebar = useAppStore((s) => s.toggleSidebar);
  const addControlTab = useAppStore((s) => s.addControlTab);
  const removeControlTab = useAppStore((s) => s.removeControlTab);
  const activeControlTabId = useAppStore((s) => s.activeControlTabId);
  const lang = useAppStore((s) => s.lang);

  const loadSettings = useSettingsStore((s) => s.load);
  const openSettings = useSettingsStore((s) => s.open);
  const openAbout = useSettingsStore((s) => s.openAbout);
  const isAboutOpen = useSettingsStore((s) => s.isAboutOpen);
  const closeAbout = useSettingsStore((s) => s.closeAbout);

  const settingsLoaded = useSettingsStore((s) => s.loaded);
  const showOnboarding = useSettingsStore((s) => s.settings.general.showOnboarding);
  const hasOpenedOnboarding = useOnboardingStore((s) => s.hasOpenedThisSession);
  const openOnboarding = useOnboardingStore((s) => s.openWizard);

  // 布局编排 (侧边栏停靠; 中央区模块树由 dockStore 负责)
  const sidebarDock = useLayoutStore((s) => s.sidebarDock);
  const draggingSidebar = useLayoutStore((s) => s.draggingSidebar);
  const setSidebarDock = useLayoutStore((s) => s.setSidebarDock);
  const setDraggingSidebar = useLayoutStore((s) => s.setDraggingSidebar);
  // 侧边栏拖拽时, 窗口左右边缘的停靠投放区高亮
  const [dockEdge, setDockEdge] = useState<'left' | 'right' | null>(null);

  // 全局默认右键菜单
  const defaultMenuItems = useMemo(
    () => [
      {
        id: 'settings',
        label: t(lang, 'settings'),
        icon: <Settings />,
        shortcut: 'Ctrl+,',
        onClick: openSettings,
      },
      {
        id: 'about',
        label: t(lang, 'about'),
        icon: <Info />,
        onClick: openAbout,
      },
      { kind: 'separator' as const },
      {
        id: 'refresh-ports',
        label: t(lang, 'refreshPorts'),
        icon: <RefreshCw />,
        onClick: () => refreshPorts(),
      },
      {
        id: 'toggle-sidebar',
        label: sidebarVisible ? t(lang, 'contextMenuHideSidebar') : t(lang, 'contextMenuShowSidebar'),
        icon: <PanelLeft />,
        onClick: () => toggleSidebar(sidebarView),
      },
    ],
    [lang, openSettings, openAbout, refreshPorts, sidebarVisible, sidebarView, toggleSidebar]
  );
  const onAppContextMenu = useContextMenu(defaultMenuItems);

  // 启动: 加载设置 + 初始化事件监听 + 刷新端口
  useEffect(() => {
    void loadSettings();
    const cleanupRef: { fn: (() => void) | null } = { fn: null };
    let cancelled = false;
    initEventListeners().then((fn) => {
      if (cancelled) {
        fn();
      } else {
        cleanupRef.fn = fn;
      }
    });
    refreshPorts();

    // 首次启动种子: widgets/tabs/nodes 均为内存态不持久化, 默认放一个 RawData 控件
    // 以保留旧版固定 raw Tab 的常驻行为 (画布占位节点 + raw 数据 Tab)
    const st = useAppStore.getState();
    if (st.widgets.length === 0 && !st.dataTabs.some((t) => t.type === 'raw')) {
      st.addWidget(createWidget('RawData'), 'default', { x: 420, y: 120 });
    }

    return () => {
      cancelled = true;
      cleanupRef.fn?.();
    };
  }, [initEventListeners, refreshPorts, loadSettings]);

  // 设置加载完成后，根据 showOnboarding 自动弹出首次引导（仅一次）
  useEffect(() => {
    if (settingsLoaded && showOnboarding && !hasOpenedOnboarding) {
      openOnboarding();
    }
  }, [settingsLoaded, showOnboarding, hasOpenedOnboarding, openOnboarding]);

  // 监听原生菜单事件 (menu:about / menu:settings / menu:new-tab / menu:close-tab / menu:toggle-sidebar)
  useEffect(() => {
    const unlistenProm = listen<string>('menu-event', (event) => {
      const id = event.payload;
      switch (id) {
        case 'menu:about':
          useSettingsStore.getState().openAbout();
          break;
        case 'menu:settings':
          openSettings();
          break;
        case 'menu:new-tab':
          addControlTab();
          break;
        case 'menu:close-tab':
          removeControlTab(activeControlTabId);
          break;
        case 'menu:toggle-sidebar':
          toggleSidebar(sidebarView);
          break;
        default:
          break;
      }
    });
    return () => {
      void unlistenProm.then((fn) => fn());
    };
  }, [openSettings, addControlTab, removeControlTab, activeControlTabId, toggleSidebar, sidebarView]);

  // 全局快捷键: Cmd+, / Ctrl+, 打开设置
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === ',') {
        e.preventDefault();
        openSettings();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [openSettings]);

  // 中央区 — Dock 布局树 (卡片可拆分/合并/重排, 尺寸比例跟随卡片)
  const centerNode = (
    <Panel key="center" id="center" order={sidebarDock === 'left' ? 2 : 1} className="min-w-0">
      <DockLayout />
    </Panel>
  );
  const sidebarNode = sidebarVisible ? (
    <Panel key="sidebar" id="sidebar" order={sidebarDock === 'left' ? 1 : 2} defaultSize={18} minSize={12} maxSize={35}>
      <div className="module-card h-full w-full">
        <Sidebar view={sidebarView} />
      </div>
    </Panel>
  ) : null;
  const mainHandle = sidebarVisible ? (
    <PanelResizeHandle
      key="main-handle"
      className="w-1 rounded-full bg-transparent hover:bg-accent/50 transition-colors"
    />
  ) : null;

  return (
    <div className="relative flex h-full flex-col bg-bg-activity p-1" onContextMenu={onAppContextMenu}>
      <div className="flex flex-1 min-h-0 gap-1">
        <div className="module-card w-12 flex-shrink-0">
          <ActivityBar
            activeView={sidebarVisible ? sidebarView : null}
            onSelect={toggleSidebar}
          />
        </div>
        <div className="flex-1 min-w-0">
          <PanelGroup key={sidebarDock} direction="horizontal" autoSaveId="sp-main" className="gap-1">
            {(sidebarDock === 'left'
              ? [sidebarNode, mainHandle, centerNode]
              : [centerNode, mainHandle, sidebarNode]
            ).filter(Boolean)}
          </PanelGroup>
        </div>
      </div>
      <div className="module-card flex-shrink-0 mt-1">
        <StatusBar />
      </div>

      {/* 侧边栏拖拽时: 窗口左右边缘的停靠投放区 */}
      {draggingSidebar && (
        <>
          {(['left', 'right'] as const).map((edge) => (
            <div
              key={edge}
              className={`absolute top-0 bottom-0 w-20 z-40 ${edge === 'left' ? 'left-0' : 'right-0'}`}
              onDragOver={(e) => {
                e.preventDefault();
                e.dataTransfer.dropEffect = 'move';
                setDockEdge(edge);
              }}
              onDragLeave={() => setDockEdge(null)}
              onDrop={(e) => {
                e.preventDefault();
                setSidebarDock(edge);
                setDockEdge(null);
                setDraggingSidebar(false);
              }}
            />
          ))}
          {dockEdge && (
            <div
              className="snap-drop-zone"
              style={{
                top: 6,
                left: dockEdge === 'left' ? 6 : '82%',
                width: '18%',
                height: 'calc(100% - 12px)',
              }}
            />
          )}
        </>
      )}

      <ContextMenu />
      <NotificationToasts />
      <SettingsModal />
      <AboutModal isOpen={isAboutOpen} onClose={closeAbout} />
      <CustomWidgetEditorContainer />
      <OnboardingWizard />
      <HelpCenterModal />
    </div>
  );
}

export default App;
