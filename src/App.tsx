import { lazy, Suspense, useEffect, useMemo, type ReactNode } from 'react';
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
import { useHistoryStore } from './store/historyStore';
import { useSettingsStore } from './store/settingsStore';
import { useOnboardingStore } from './store/onboardingStore';
import { useLayoutStore, AI_FLOAT_MIN_W, AI_FLOAT_MIN_H } from './store/layoutStore';
import { AiChatPanel } from './components/ai/AiChatPanel';
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

  // 布局编排 (侧边栏与 AI 面板停靠; 中央区模块树由 dockStore 负责)
  const sidebarDock = useLayoutStore((s) => s.sidebarDock);
  const draggingSidebar = useLayoutStore((s) => s.draggingSidebar);
  const dockEdgeHover = useLayoutStore((s) => s.dockEdgeHover);
  const aiPanelVisible = useLayoutStore((s) => s.aiPanelVisible);
  const aiDock = useLayoutStore((s) => s.aiDock);
  const aiFloatRect = useLayoutStore((s) => s.aiFloatRect);
  const draggingAiPanel = useLayoutStore((s) => s.draggingAiPanel);
  const aiDockEdgeHover = useLayoutStore((s) => s.aiDockEdgeHover);

  // 浮动窗口渲染位置 clamp 到窗口内 (窗口缩小后仍可见; store 原值不动)
  const floatPos = useMemo(() => {
    const maxX = Math.max(8, window.innerWidth - aiFloatRect.w - 8);
    const maxY = Math.max(8, window.innerHeight - aiFloatRect.h - 8);
    return {
      x: Math.min(Math.max(aiFloatRect.x, 8), maxX),
      y: Math.min(Math.max(aiFloatRect.y, 8), maxY),
    };
  }, [aiFloatRect.x, aiFloatRect.y, aiFloatRect.w, aiFloatRect.h]);

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

  // 全局快捷键: 文档域撤销/重做 — Ctrl/Cmd+Z / Ctrl+Y (Shift 反转 Z 方向)。
  // 焦点在文本输入类元素 (input/textarea/contentEditable, 含 CodeMirror) 时
  // 直接放行 — 保留各输入控件自身的原生文本撤销, 与文档域历史互不干扰。
  useEffect(() => {
    const isEditableTarget = (target: EventTarget | null): boolean => {
      if (!(target instanceof HTMLElement)) return false;
      return (
        target.tagName === 'INPUT' ||
        target.tagName === 'TEXTAREA' ||
        target.tagName === 'SELECT' ||
        target.isContentEditable
      );
    };
    const handler = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.altKey) return;
      const key = e.key.toLowerCase();
      if (key !== 'z' && key !== 'y') return;
      if (isEditableTarget(e.target)) return; // 文本框内: 原生文本撤销优先
      const history = useHistoryStore.getState();
      e.preventDefault();
      if (key === 'y' || e.shiftKey) {
        history.redo();
      } else {
        history.undo();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, []);

  // 主横向面板组 — 按停靠位置拼装: [左侧停靠面板…] 中央 Dock [右侧停靠面板…]
  // (AI 面板 left/right 与侧边栏同级; bottom/float 在下方单独处理)
  const aiDockedSide = aiPanelVisible && (aiDock === 'left' || aiDock === 'right') ? aiDock : null;
  const sidebarSide = sidebarVisible ? sidebarDock : null;
  const bottomDocked = aiPanelVisible && aiDock === 'bottom';
  const handleClass = 'w-2 rounded-full bg-transparent hover:bg-accent/50 transition-colors';

  const mainGroupItems: ReactNode[] = [];
  let panelOrder = 0;
  const nextOrder = () => ++panelOrder;
  const resizeHandle = (key: string) => (
    <PanelResizeHandle key={key} className={handleClass} />
  );
  const sidebarPanel = (ord: number) => (
    <Panel key="sidebar" id="sidebar" order={ord} defaultSize={18} minSize={12} maxSize={35}>
      <div className="module-card h-full w-full">
        <Sidebar view={sidebarView} />
      </div>
    </Panel>
  );
  const aiPanelNode = (ord: number) => (
    <Panel key="ai-panel" id="ai-panel" order={ord} defaultSize={24} minSize={14} maxSize={55}>
      <div className="module-card h-full w-full">
        <AiChatPanel />
      </div>
    </Panel>
  );
  if (sidebarSide === 'left') mainGroupItems.push(sidebarPanel(nextOrder()), resizeHandle('main-handle-l'));
  if (aiDockedSide === 'left') mainGroupItems.push(aiPanelNode(nextOrder()), resizeHandle('ai-handle-l'));
  mainGroupItems.push(
    <Panel key="center" id="center" order={nextOrder()} className="min-w-0">
      <DockLayout />
    </Panel>
  );
  if (aiDockedSide === 'right') mainGroupItems.push(resizeHandle('ai-handle-r'), aiPanelNode(nextOrder()));
  if (sidebarSide === 'right') mainGroupItems.push(resizeHandle('main-handle-r'), sidebarPanel(nextOrder()));

  // 工作台行 — bottom 停靠时被纵向 PanelGroup 包裹, 否则直接占满剩余高度
  const workbench = (
    <div className={`flex min-h-0 gap-2 ${bottomDocked ? 'h-full' : 'flex-1'}`} data-tour="tour-workbench">
      <div className="module-card w-12 flex-shrink-0">
        <ActivityBar
          activeView={sidebarVisible ? sidebarView : null}
          onSelect={toggleSidebar}
        />
      </div>
      <div className="flex-1 min-w-0">
        <PanelGroup key={`${sidebarDock}:${aiDock}`} direction="horizontal" autoSaveId="sp-main">
          {mainGroupItems}
        </PanelGroup>
      </div>
    </div>
  );

  return (
    <div className="relative flex h-full flex-col bg-bg-window p-2" onContextMenu={onAppContextMenu}>
      {/* Windows: 透明窗口下原生菜单栏无法正确渲染 (白字白底/透视), 由自定义菜单栏接管; 其它平台内部返回 null */}
      <MenuBar />
      {bottomDocked ? (
        // AI 面板停靠底部 — 工作台与对话面板纵向分栏
        <PanelGroup direction="vertical" autoSaveId="sp-ai-bottom" className="flex-1 min-h-0">
          <Panel minSize={30} className="min-h-0">
            {workbench}
          </Panel>
          <PanelResizeHandle className="h-2 rounded-full bg-transparent hover:bg-accent/50 transition-colors" />
          <Panel defaultSize={34} minSize={15} maxSize={70} className="min-h-0">
            <div className="module-card h-full w-full">
              <AiChatPanel />
            </div>
          </Panel>
        </PanelGroup>
      ) : (
        workbench
      )}
      {/* AI 面板浮动 — 标题栏拖动重停靠 (dockDrag), 右下角把手调尺寸 */}
      {aiPanelVisible && aiDock === 'float' && (
        <div
          className="absolute z-50"
          style={{ left: floatPos.x, top: floatPos.y, width: aiFloatRect.w, height: aiFloatRect.h }}
        >
          <div className="module-card h-full w-full shadow-2xl">
            <AiChatPanel />
          </div>
          <div
            className="absolute -right-0.5 -bottom-0.5 w-3.5 h-3.5 cursor-nwse-resize rounded-sm hover:bg-accent/60"
            onPointerDown={(e) => {
              if (e.button !== 0) return;
              e.preventDefault();
              const startX = e.clientX;
              const startY = e.clientY;
              const start = useLayoutStore.getState().aiFloatRect;
              const onMove = (ev: PointerEvent) => {
                useLayoutStore.getState().setAiFloatRect({
                  x: start.x,
                  y: start.y,
                  w: Math.max(AI_FLOAT_MIN_W, start.w + ev.clientX - startX),
                  h: Math.max(AI_FLOAT_MIN_H, start.h + ev.clientY - startY),
                });
              };
              const onUp = () => {
                window.removeEventListener('pointermove', onMove);
                window.removeEventListener('pointerup', onUp);
              };
              window.addEventListener('pointermove', onMove);
              window.addEventListener('pointerup', onUp);
            }}
          />
        </div>
      )}
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

      {/* AI 面板拖拽时: 左/右/下边缘热区 + 松手空白处浮动 */}
      {draggingAiPanel && (
        <>
          {(['left', 'right', 'bottom'] as const).map((edge) => (
            <div
              key={edge}
              data-dock-zone="ai-dock"
              data-dock-edge={edge}
              className={
                edge === 'bottom'
                  ? 'absolute left-0 right-0 bottom-0 h-24 z-40'
                  : `absolute top-0 bottom-0 w-20 z-40 ${edge === 'left' ? 'left-0' : 'right-0'}`
              }
            />
          ))}
          {aiDockEdgeHover && (
            <div
              className="snap-drop-zone visible"
              style={
                aiDockEdgeHover === 'left'
                  ? { top: 6, left: 6, width: '18%', height: 'calc(100% - 12px)' }
                  : aiDockEdgeHover === 'right'
                    ? { top: 6, left: '82%', width: '18%', height: 'calc(100% - 12px)' }
                    : { bottom: 6, left: 6, width: 'calc(100% - 12px)', height: '32%' }
              }
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
