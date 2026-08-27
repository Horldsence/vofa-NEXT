import { lazy, Suspense, useEffect, useMemo } from 'react';
import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { Settings, Info, RefreshCw, PanelLeft } from 'lucide-react';
import { ActivityBar } from './components/layout/ActivityBar';
import { Sidebar } from './components/layout/Sidebar';
import { DockLayout } from './components/layout/DockLayout';
import { StatusBar } from './components/layout/StatusBar';
import { MenuBar } from './components/layout/MenuBar';
import { NotificationToasts } from './components/NotificationToasts';
import { ContextMenu } from './components/ui/ContextMenu';
import { DockDragGhost } from './components/ui/DockDragGhost';
import { SuspenseFallback } from './components/ui/SuspenseFallback';
import { useContextMenu } from './lib/hooks/useContextMenu';
import { useAppStore } from './store/appStore';
import { useSettingsStore } from './store/settingsStore';
import { useOnboardingStore } from './store/onboardingStore';
import { useLayoutStore } from './store/layoutStore';
import { useUpdateStore } from './store/updateStore';
import { t } from './i18n';
import { createWidget } from './lib/utils/createWidget';
import { openDataPanelAndReveal } from './lib/utils/revealDataTab';

// 重型弹窗 — 懒加载 (各模块仅含 named export, 经 *.lazy.tsx 包装为 default)
const SettingsModal = lazy(() => import('./components/SettingsModal.lazy'));
const AboutModal = lazy(() => import('./components/AboutModal.lazy'));
const CustomWidgetEditorContainer = lazy(() => import('./components/CustomWidgetEditorContainer.lazy'));
const OnboardingWizard = lazy(() => import('./components/onboarding/OnboardingWizard.lazy'));
const HelpCenterModal = lazy(() => import('./components/onboarding/HelpCenterModal.lazy'));
const UpdateDialog = lazy(() => import('./components/UpdateDialog.lazy'));

function App() {
  const initEventListeners = useAppStore((s) => s.initEventListeners);
  const refreshPorts = useAppStore((s) => s.refreshPorts);
  const sidebarView = useAppStore((s) => s.sidebarView);
  const sidebarVisible = useAppStore((s) => s.sidebarVisible);
  const toggleSidebar = useAppStore((s) => s.toggleSidebar);
  const lang = useAppStore((s) => s.lang);

  const loadSettings = useSettingsStore((s) => s.load);
  const openSettings = useSettingsStore((s) => s.open);
  const openAbout = useSettingsStore((s) => s.openAbout);
  const isAboutOpen = useSettingsStore((s) => s.isAboutOpen);
  const closeAbout = useSettingsStore((s) => s.closeAbout);
  const isSettingsOpen = useSettingsStore((s) => s.isOpen);

  const settingsLoaded = useSettingsStore((s) => s.loaded);
  const showOnboarding = useSettingsStore((s) => s.settings.general.showOnboarding);
  const autoCheckUpdate = useSettingsStore((s) => s.settings.general.autoCheckUpdate);
  const updateDialogOpen = useUpdateStore((s) => s.dialogOpen);
  const statusBarVisible = useSettingsStore((s) => s.settings.appearance.statusBarVisible);
  const hasOpenedOnboarding = useOnboardingStore((s) => s.hasOpenedThisSession);
  const openOnboarding = useOnboardingStore((s) => s.openWizard);
  const isWizardOpen = useOnboardingStore((s) => s.isWizardOpen);
  const isHelpOpen = useOnboardingStore((s) => s.isHelpOpen);
  const isCustomEditorOpen = useAppStore((s) => s.customEditorState.open);

  // 布局编排 (侧边栏停靠; 中央区模块树由 dockStore 负责)
  const sidebarDock = useLayoutStore((s) => s.sidebarDock);
  const draggingSidebar = useLayoutStore((s) => s.draggingSidebar);
  const dockEdgeHover = useLayoutStore((s) => s.dockEdgeHover);

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
  // 依赖均为 store 动作 (引用稳定), 只需挂载时执行一次
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
      const rawWidget = createWidget('RawData');
      st.addWidget(rawWidget, 'default', { x: 560, y: 120 });
      // 空图时再给出 设备→协议解析→RawData 初始连线, 新用户开箱即有完整数据通路
      const fresh = useAppStore.getState();
      if (!fresh.rfNodes.some((n) => n.data?.global === true)) {
        fresh.seedInitialGraph(rawWidget.params.id);
      }
    }

    return () => {
      cancelled = true;
      cleanupRef.fn?.();
    };
  }, []);

  // 设置加载完成后: 关闭启动页并显示主窗口
  // 注意不能用 requestAnimationFrame 等待首帧 — 窗口隐藏时 rAF 会被系统节流不触发
  useEffect(() => {
    if (!settingsLoaded) return;
    void invoke('close_splashscreen');
  }, [settingsLoaded]);

  // 设置加载完成后，根据 showOnboarding 自动弹出首次引导（仅一次）
  useEffect(() => {
    if (settingsLoaded && showOnboarding && !hasOpenedOnboarding) {
      openOnboarding();
    }
  }, [settingsLoaded, showOnboarding, hasOpenedOnboarding, openOnboarding]);

  // 设置加载完成后: 自动检查更新 (仅一次; auto 失败不打断用户)
  useEffect(() => {
    if (!settingsLoaded || !autoCheckUpdate) return;
    void useUpdateStore.getState().check('auto');
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settingsLoaded]);

  // 监听原生菜单事件 (menu:about / menu:settings / menu:new-tab / menu:close-tab / menu:toggle-sidebar)
  // 事件处理器内通过 getState() 读取最新状态, 避免订阅易变字段导致反复重订阅
  useEffect(() => {
    const unlistenProm = listen<string>('menu-event', (event) => {
      const id = event.payload;
      switch (id) {
        case 'menu:about':
          useSettingsStore.getState().openAbout();
          break;
        case 'menu:settings':
          useSettingsStore.getState().open();
          break;
        case 'menu:new-tab':
          useAppStore.getState().addControlTab();
          break;
        case 'menu:close-tab':
          useAppStore.getState().removeControlTab(useAppStore.getState().activeControlTabId);
          break;
        case 'menu:toggle-sidebar': {
          const st = useAppStore.getState();
          st.toggleSidebar(st.sidebarView);
          break;
        }
        // 数据面板菜单 (Panel) — 与 src-tauri/crates/menu_shell 的 ids::PANEL_OPEN_* 一一对应;
        // 打开后同步把目标 Tab 所在 data 卡片切过去 (与侧边栏/MenuBar 入口行为一致)
        case 'menu:panel-open-compile-errors':
          openDataPanelAndReveal(() => useAppStore.getState().addCompileErrorsTab());
          break;
        case 'menu:panel-open-compile-results':
          openDataPanelAndReveal(() => useAppStore.getState().addCompileResultsTab());
          break;
        case 'menu:panel-open-can':
          openDataPanelAndReveal(() => useAppStore.getState().addCanTab());
          break;
        case 'menu:panel-open-logic':
          openDataPanelAndReveal(() => useAppStore.getState().addLogicTab());
          break;
        default:
          break;
      }
    });
    return () => {
      void unlistenProm.then((fn) => fn());
    };
  }, []);

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
  }, []);

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
      className="w-2 rounded-full bg-transparent hover:bg-accent/50 transition-colors"
    />
  ) : null;

  return (
    <div className="relative flex h-full flex-col bg-bg-window p-2" onContextMenu={onAppContextMenu}>
      {/* Windows: 透明窗口下原生菜单栏无法正确渲染 (白字白底/透视), 由自定义菜单栏接管; 其它平台内部返回 null */}
      <MenuBar />
      <div className="flex flex-1 min-h-0 gap-2">
        <div className="module-card w-12 flex-shrink-0">
          <ActivityBar
            activeView={sidebarVisible ? sidebarView : null}
            onSelect={toggleSidebar}
          />
        </div>
        <div className="flex-1 min-w-0">
          <PanelGroup key={sidebarDock} direction="horizontal" autoSaveId="sp-main">
            {(sidebarDock === 'left'
              ? [sidebarNode, mainHandle, centerNode]
              : [centerNode, mainHandle, sidebarNode]
            ).filter(Boolean)}
          </PanelGroup>
        </div>
      </div>
      {statusBarVisible && (
        <div className="module-card flex-shrink-0 mt-2">
          <StatusBar />
        </div>
      )}

      {/* 侧边栏拖拽时: 窗口左右边缘的停靠投放区 — dockDrag 控制器按指针命中测试 */}
      {draggingSidebar && (
        <>
          {(['left', 'right'] as const).map((edge) => (
            <div
              key={edge}
              data-dock-zone="sidebar-dock"
              data-dock-edge={edge}
              className={`absolute top-0 bottom-0 w-20 z-40 ${edge === 'left' ? 'left-0' : 'right-0'}`}
            />
          ))}
          {dockEdgeHover && (
            <div
              className="snap-drop-zone visible"
              style={{
                top: 6,
                left: dockEdgeHover === 'left' ? 6 : '82%',
                width: '18%',
                height: 'calc(100% - 12px)',
              }}
            />
          )}
        </>
      )}

      <ContextMenu />
      <DockDragGhost />
      <NotificationToasts />
      {/* 弹窗按需挂载: 打开时才触发懒加载, 避免启动时 5 个 lazy chunk 并发加载导致全屏遮罩闪烁 */}
      {isSettingsOpen && (
        <Suspense fallback={<SuspenseFallback overlay />}>
          <SettingsModal />
        </Suspense>
      )}
      {isAboutOpen && (
        <Suspense fallback={<SuspenseFallback overlay />}>
          <AboutModal isOpen={isAboutOpen} onClose={closeAbout} />
        </Suspense>
      )}
      {isCustomEditorOpen && (
        <Suspense fallback={<SuspenseFallback overlay />}>
          <CustomWidgetEditorContainer />
        </Suspense>
      )}
      {isWizardOpen && (
        <Suspense fallback={<SuspenseFallback overlay />}>
          <OnboardingWizard />
        </Suspense>
      )}
      {isHelpOpen && (
        <Suspense fallback={<SuspenseFallback overlay />}>
          <HelpCenterModal />
        </Suspense>
      )}
      {updateDialogOpen && (
        <Suspense fallback={<SuspenseFallback overlay />}>
          <UpdateDialog />
        </Suspense>
      )}
    </div>
  );
}

export default App;
