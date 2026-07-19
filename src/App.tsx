import { useEffect, useMemo } from 'react';
import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels';
import { listen } from '@tauri-apps/api/event';
import { Settings, Info, RefreshCw, PanelLeft } from 'lucide-react';
import { ActivityBar } from './components/layout/ActivityBar';
import { Sidebar } from './components/layout/Sidebar';
import { ControlPanel } from './components/layout/ControlPanel';
import { DataPanel } from './components/layout/DataPanel';
import { StatusBar } from './components/layout/StatusBar';
import { NotificationToasts } from './components/NotificationToasts';
import { SettingsModal } from './components/SettingsModal';
import { AboutModal } from './components/AboutModal';
import { CustomWidgetEditorContainer } from './components/CustomWidgetEditorContainer';
import { OnboardingWizard } from './components/onboarding/OnboardingWizard';
import { HelpCenterModal } from './components/onboarding/HelpCenterModal';
import { ContextMenu } from './components/ui/ContextMenu';
import { useContextMenu } from './lib/useContextMenu';
import { useAppStore } from './store/appStore';
import { useSettingsStore } from './store/settingsStore';
import { useOnboardingStore } from './store/onboardingStore';
import { t } from './i18n';

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

  return (
    <div className="flex h-full flex-col" onContextMenu={onAppContextMenu}>
      <div className="flex flex-1 min-h-0">
        <ActivityBar
          activeView={sidebarVisible ? sidebarView : null}
          onSelect={toggleSidebar}
        />
        <PanelGroup direction="horizontal" autoSaveId="sp-main">
          {sidebarVisible && (
            <>
              <Panel defaultSize={18} minSize={12} maxSize={35} order={1}>
                <Sidebar view={sidebarView} />
              </Panel>
              <PanelResizeHandle className="w-px bg-border cursor-col-resize" />
            </>
          )}
          <Panel order={2}>
            <PanelGroup direction="vertical" autoSaveId="sp-center">
              <Panel defaultSize={45} minSize={15} order={1}>
                <ControlPanel />
              </Panel>
              <PanelResizeHandle className="h-px bg-border cursor-row-resize" />
              <Panel defaultSize={55} minSize={15} order={2}>
                <DataPanel />
              </Panel>
            </PanelGroup>
          </Panel>
        </PanelGroup>
      </div>
      <StatusBar />
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
