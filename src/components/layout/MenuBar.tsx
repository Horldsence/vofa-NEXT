import { memo, useEffect, useMemo, useState } from 'react';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { openUrl } from '@tauri-apps/plugin-opener';
import { exit } from '@tauri-apps/plugin-process';
import clsx from 'clsx';
import { useAppStore } from '../../store/appStore';
import { useHistoryStore } from '../../store/historyStore';
import { useSettingsStore } from '../../store/settingsStore';
import { getAvailableDataPanelEntries, type DataPanelEntry } from '../../store/slices/dataTabs';
import { openDataPanelAndReveal } from '../../lib/utils/revealDataTab';
import { t, type Lang } from '../../i18n';

const APP_GITHUB = 'https://github.com/Horldsence/vofa-NEXT';
const APP_DOCS = 'https://github.com/Horldsence/vofa-NEXT#readme';

/// Windows 平台检测 — 仅 Windows 挂载自定义菜单栏。
///
/// 背景: 主窗口为透明窗口 (WS_EX_LAYERED), Windows 原生菜单栏在分层窗口上
/// 无法正确绘制 — 菜单文字按深色模式渲染为白色, 但菜单背景不填充, 造成
/// "白字白底" 且背景透视露出下层内容。因此 Windows 端原生菜单已停用
/// (见 src-tauri/src/lib.rs setup), 由本组件复刻同等菜单。
export function isWindowsPlatform(): boolean {
  return typeof navigator !== 'undefined' && /Windows/i.test(navigator.userAgent);
}

/// 缩放系数 — 模块级持久, 跨菜单重开保留 (与原生菜单语义一致)
const ZOOM_STEP = 0.2;
const ZOOM_MIN = 0.5;
const ZOOM_MAX = 3;
let zoomFactor = 1;

async function setZoom(next: number): Promise<void> {
  zoomFactor = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, next));
  try {
    await getCurrentWebviewWindow().setZoom(zoomFactor);
  } catch {
    // 纯浏览器 dev / 测试环境无 Tauri 后端, 静默忽略
  }
}

/// Edit 菜单命令 — Chromium (WebView2) 仍支持 execCommand, 作用于当前焦点元素
function execEditCommand(cmd: 'undo' | 'redo' | 'cut' | 'copy' | 'paste' | 'selectAll'): void {
  try {
    document.execCommand(cmd);
  } catch {
    // 忽略
  }
}

interface MenuAction {
  kind: 'item';
  id: string;
  label: string;
  shortcut?: string;
  disabled?: boolean;
  onClick: () => void;
}

interface MenuSeparator {
  kind: 'separator';
}

type MenuEntry = MenuAction | MenuSeparator;

interface MenuDef {
  id: string;
  label: string;
  entries: MenuEntry[];
}

/// 构建菜单定义 — 与 Rust 原生菜单 (src-tauri/src/menu.rs) 保持同等结构
function buildMenus(lang: Lang, historyFlags: { canUndo: boolean; canRedo: boolean }): MenuDef[] {
  const L = (key: string) => t(lang, key);
  const app = useAppStore.getState;
  const settings = useSettingsStore.getState;

  const newTab = () => app().addControlTab();
  const closeTab = () => {
    const s = app();
    s.removeControlTab(s.activeControlTabId);
  };
  const toggleSidebar = () => {
    const s = app();
    s.toggleSidebar(s.sidebarView);
  };

  /// 数据面板菜单条目 — 复用 dataTabs slice 的单一事实源;
  /// 派生面板项随画布 widgets 变化 enabled/disabled
  const entries = getAvailableDataPanelEntries(app(), app());
  const entryToMenu = (e: DataPanelEntry): MenuAction => ({
    kind: 'item',
    id: `panel-${e.type}`,
    label: e.available ? L(e.labelKey) : `${L(e.labelKey)} (${L('panelOpenNoWidget')})`,
    disabled: !e.available,
    onClick: () => openDataPanelAndReveal(e.open),
  });
  const standalone = entries.filter((e) => e.group === 'standalone').map(entryToMenu);
  const derived = entries.filter((e) => e.group === 'derived').map(entryToMenu);

  return [
    {
      id: 'app',
      label: 'VOFA-Next',
      entries: [
        { kind: 'item', id: 'about', label: L('menuAbout'), onClick: () => settings().openAbout() },
        { kind: 'item', id: 'settings', label: L('menuSettings'), shortcut: 'Ctrl+,', onClick: () => settings().open() },
        { kind: 'separator' },
        { kind: 'item', id: 'quit', label: L('menuQuit'), onClick: () => void exit(0).catch(() => { return undefined; }) },
      ],
    },
    {
      id: 'file',
      label: L('menuFile'),
      entries: [
        { kind: 'item', id: 'new-tab', label: L('menuNewTab'), shortcut: 'Ctrl+T', onClick: newTab },
        { kind: 'item', id: 'close-tab', label: L('menuCloseTab'), shortcut: 'Ctrl+W', onClick: closeTab },
      ],
    },
    {
      id: 'panel',
      label: L('menuPanel'),
      entries: [
        ...standalone,
        ...(standalone.length && derived.length ? [{ kind: 'separator' as const }] : []),
        ...derived,
      ],
    },
    {
      id: 'edit',
      label: L('menuEdit'),
      entries: [
        {
          kind: 'item',
          id: 'undo',
          label: L('menuUndo'),
          shortcut: 'Ctrl+Z',
          disabled: !historyFlags.canUndo,
          onClick: () => useHistoryStore.getState().undo(),
        },
        {
          kind: 'item',
          id: 'redo',
          label: L('menuRedo'),
          shortcut: 'Ctrl+Y',
          disabled: !historyFlags.canRedo,
          onClick: () => useHistoryStore.getState().redo(),
        },
        { kind: 'separator' },
        { kind: 'item', id: 'cut', label: L('menuCut'), shortcut: 'Ctrl+X', onClick: () => execEditCommand('cut') },
        { kind: 'item', id: 'copy', label: L('menuCopy'), shortcut: 'Ctrl+C', onClick: () => execEditCommand('copy') },
        { kind: 'item', id: 'paste', label: L('menuPaste'), shortcut: 'Ctrl+V', onClick: () => execEditCommand('paste') },
        { kind: 'item', id: 'select-all', label: L('menuSelectAll'), shortcut: 'Ctrl+A', onClick: () => execEditCommand('selectAll') },
      ],
    },
    {
      id: 'view',
      label: L('menuView'),
      entries: [
        { kind: 'item', id: 'toggle-sidebar', label: L('menuToggleSidebar'), shortcut: 'Ctrl+B', onClick: toggleSidebar },
        { kind: 'separator' },
        { kind: 'item', id: 'reload', label: L('menuReload'), shortcut: 'Ctrl+R', onClick: () => window.location.reload() },
        { kind: 'item', id: 'zoom-in', label: L('menuZoomIn'), shortcut: 'Ctrl+=', onClick: () => void setZoom(zoomFactor + ZOOM_STEP) },
        { kind: 'item', id: 'zoom-out', label: L('menuZoomOut'), shortcut: 'Ctrl+-', onClick: () => void setZoom(zoomFactor - ZOOM_STEP) },
        { kind: 'item', id: 'zoom-reset', label: L('menuZoomReset'), shortcut: 'Ctrl+0', onClick: () => void setZoom(1) },
      ],
    },
    {
      id: 'window',
      label: L('menuWindow'),
      entries: [
        {
          kind: 'item',
          id: 'minimize',
          label: L('menuMinimize'),
          onClick: () => void getCurrentWebviewWindow().minimize().catch(() => { return undefined; }),
        },
        {
          kind: 'item',
          id: 'toggle-maximize',
          label: L('menuMaximize'),
          onClick: () => void getCurrentWebviewWindow().toggleMaximize().catch(() => { return undefined; }),
        },
        {
          kind: 'item',
          id: 'close-window',
          label: L('menuCloseWindow'),
          onClick: () => void getCurrentWebviewWindow().close().catch(() => { return undefined; }),
        },
      ],
    },
    {
      id: 'help',
      label: L('menuHelp'),
      entries: [
        { kind: 'item', id: 'docs', label: L('menuDocs'), onClick: () => void openUrl(APP_DOCS).catch(() => { return undefined; }) },
        { kind: 'item', id: 'github', label: L('menuGithub'), onClick: () => void openUrl(APP_GITHUB).catch(() => { return undefined; }) },
        { kind: 'separator' },
        { kind: 'item', id: 'about', label: L('menuAbout'), onClick: () => settings().openAbout() },
      ],
    },
  ];
}

/// Windows 自定义菜单栏 — 复刻原生菜单 (App/File/Edit/View/Window/Help)
///
/// 交互:
/// - 点击菜单标题开合下拉面板; 已打开时悬停其它标题直接切换
/// - 空白区域 (data-tauri-drag-region) 可拖动窗口, 双击最大化
/// - 下拉面板复用 context-menu 视觉样式, 保证深色主题一致
export const MenuBar = memo(function MenuBar() {
  const lang = useAppStore((s) => s.lang);
  const canUndo = useHistoryStore((s) => s.canUndo);
  const canRedo = useHistoryStore((s) => s.canRedo);
  const [openMenuId, setOpenMenuId] = useState<string | null>(null);
  const menus = useMemo(() => buildMenus(lang, { canUndo, canRedo }), [lang, canUndo, canRedo]);
  const isWindows = useMemo(isWindowsPlatform, []);

  // Windows 平台快捷键 — 原生菜单已停用, 此处接管加速键
  useEffect(() => {
    if (!isWindowsPlatform()) return;
    const onKeyDown = (e: KeyboardEvent) => {
      // 仅响应 Ctrl+key (不含 Alt/Meta), 避免覆盖编辑快捷键
      if (!e.ctrlKey || e.altKey || e.metaKey) return;
      switch (e.key.toLowerCase()) {
        case 't':
          e.preventDefault();
          useAppStore.getState().addControlTab();
          break;
        case 'w': {
          e.preventDefault();
          const s = useAppStore.getState();
          s.removeControlTab(s.activeControlTabId);
          break;
        }
        case 'b':
          e.preventDefault();
          {
            const s = useAppStore.getState();
            s.toggleSidebar(s.sidebarView);
          }
          break;
        case 'r':
          e.preventDefault();
          window.location.reload();
          break;
        case '=':
        case '+':
          e.preventDefault();
          void setZoom(zoomFactor + ZOOM_STEP);
          break;
        case '-':
        case '_':
          e.preventDefault();
          void setZoom(zoomFactor - ZOOM_STEP);
          break;
        case '0':
          e.preventDefault();
          void setZoom(1);
          break;
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  // 外部点击 / ESC 关闭下拉面板
  useEffect(() => {
    if (openMenuId === null) return;
    const handlePointer = (e: PointerEvent | MouseEvent) => {
      const target = e.target as Node;
      // 下拉面板内部点击不关闭 (菜单项点击自行处理); 点其它任何地方 (含菜单栏空白拖动区) 关闭
      if (target instanceof Element && target.closest('[data-menu-panel]')) return;
      setOpenMenuId(null);
    };
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        setOpenMenuId(null);
      }
    };
    window.addEventListener('pointerdown', handlePointer, true);
    window.addEventListener('keydown', handleKey, true);
    return () => {
      window.removeEventListener('pointerdown', handlePointer, true);
      window.removeEventListener('keydown', handleKey, true);
    };
  }, [openMenuId]);

  const handleItemClick = (entry: MenuAction) => () => {
    entry.onClick();
    setOpenMenuId(null);
  };

  // 非 Windows 平台不渲染 — 原生菜单栏 (macOS 系统顶栏 / Linux GTK) 正常
  if (!isWindows) return null;

  return (
    <div
      data-menu-bar
      data-tauri-drag-region
      className="-mx-2 -mt-2 mb-2 flex h-7 items-stretch border-b border-border bg-bg-panel-header text-text-primary select-none relative z-50"
    >
      {menus.map((menu) => {
        const open = openMenuId === menu.id;
        return (
          <div key={menu.id} className="relative flex items-stretch">
            <button
              type="button"
              aria-haspopup="menu"
              aria-expanded={open}
              className={clsx(
                'flex items-center px-3 text-xs whitespace-nowrap transition-colors',
                open ? 'bg-bg-hover text-text-bright' : 'hover:bg-bg-hover hover:text-text-bright'
              )}
              onMouseEnter={() => {
                if (openMenuId !== null) setOpenMenuId(menu.id);
              }}
              onClick={() => setOpenMenuId(open ? null : menu.id)}
            >
              {menu.label}
            </button>
            {open && (
              <div role="menu" data-menu-panel className="context-menu absolute left-0 top-full">
                {menu.entries.map((entry, idx) => {
                  if (entry.kind === 'separator') {
                    return <div key={`sep-${idx}`} className="context-menu-separator" />;
                  }
                  return (
                    <button
                      key={entry.id}
                      type="button"
                      role="menuitem"
                      className={clsx(
                        'context-menu-item',
                        entry.disabled && 'opacity-50 cursor-not-allowed pointer-events-none'
                      )}
                      style={{ height: 26 }}
                      onClick={handleItemClick(entry)}
                    >
                      <span className="context-menu-icon" />
                      <span className="context-menu-label">{entry.label}</span>
                      {entry.shortcut && <span className="context-menu-shortcut">{entry.shortcut}</span>}
                    </button>
                  );
                })}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
});
