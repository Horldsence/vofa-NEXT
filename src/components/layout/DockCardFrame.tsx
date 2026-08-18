import { memo, useState, useCallback } from 'react';
import { Plus, X, Type, Trash2, Copy, Cpu, CircuitBoard } from 'lucide-react';
import { useShallow } from 'zustand/react/shallow';
import { useAppStore, type AppStore } from '../../store/appStore';
import { useDockStore } from '../../store/dockStore';
import { useOnboardingStore } from '../../store/onboardingStore';
import { useSettingsStore } from '../../store/settingsStore';
import { notify } from '../../lib/tauri/notifications';
import { useSlidingPill, SlidingPill } from '../ui/SlidingPill';
import { AnimatedSwitch } from '../ui/AnimatedSwitch';
import { NodeEditor } from './NodeEditor';
import { DataTabContent, DataTabIcon } from './DataTabContent';
import { useContextMenu, showContextMenu } from '../../lib/hooks/useContextMenu';
import { transitionStore } from '../../lib/utils/transitionStore';
import { dockDrag } from '../../lib/dockDrag';
import { t } from '../../i18n';

/// 通用 Dock 卡片框架 — 标题栏 (Tab 条 + 滑动指示器) + 内容区 + 吸附投放层
/// 交互 (指针事件驱动, 替代 HTML5 DnD — WKWebView 下 HTML5 拖拽不可靠):
/// - 拖动单个 Tab 到本卡片标题栏 → 合并为本卡片的一个 Tab
/// - 拖动单个 Tab 到卡片边缘 → 拆分为独立面板
/// - 拖动标题栏空白处 → 整卡移动到其他卡片边缘
export const DockCardFrame = memo(function DockCardFrame({ cardId }: { cardId: string }) {
  const lang = useAppStore((s) => s.lang);
  const card = useDockStore((s) => s.cards[cardId]);
  const addControlTab = useAppStore((s) => s.addControlTab);
  const removeControlTab = useAppStore((s) => s.removeControlTab);
  const renameControlTab = useAppStore((s) => s.renameControlTab);
  const removeDataTab = useAppStore((s) => s.removeDataTab);

  const draggingTab = useDockStore((s) => s.draggingTab);
  const setActiveTab = useDockStore((s) => s.setActiveTab);
  const setFocusedCard = useDockStore((s) => s.setFocusedCard);

  const [editingTabId, setEditingTabId] = useState<string | null>(null);
  const [editName, setEditName] = useState('');
  // 合并目标高亮 — 由 dockDrag 控制器按指针悬停写入 store
  const mergeHover = useDockStore((s) => s.mergeHoverCardId === cardId);

  // 卡片可能因树折叠在渲染间隙被移除 — hook 需无条件调用, 用兜底值
  const kind = card?.kind ?? 'data';
  const cardTabIds = card?.tabIds ?? [];
  const activeTabId = card ? (card.activeTabId ?? card.tabIds[0] ?? null) : null;

  // 本卡片承载的 Tab 列表 — 按 cardId 窄化订阅 (useShallow 顶层数组逐元素比较):
  // 其他卡片 Tab 的名称/列表变化时, 本卡片的重渲染被抑制
  const tabs: Array<{ id: string; name: string; type?: string; closable?: boolean }> = useAppStore(
    useShallow(
      ((s: AppStore): Array<{ id: string; name: string; type?: string; closable?: boolean }> =>
        kind === 'control'
          ? s.controlTabs.filter((tab) => cardTabIds.includes(tab.id))
          : s.dataTabs.filter((tab) => cardTabIds.includes(tab.id)))
    )
  );
  // 全局 Tab 数量/类型派生标量 — 窄化为布尔/数字, 仅在对应状态翻转时触发重渲染
  const canClose = useAppStore((s) => s.controlTabs.length > 1);
  const hasCanTab = useAppStore((s) => s.dataTabs.some((tab) => tab.type === 'can'));
  const hasLogicTab = useAppStore((s) => s.dataTabs.some((tab) => tab.type === 'logic'));

  // Tab 滑动指示器
  const { containerRef: tabBarRef, pill: tabPill } = useSlidingPill(activeTabId);

  // 标题栏为 Tab 合并投放目标 (仅同 kind 的跨卡片 Tab 拖拽)
  const mergeActive = draggingTab !== null && draggingTab.kind === kind && draggingTab.fromCardId !== cardId;

  // 拖拽源 — Tab 拖拽 (同 kind 拖拽时也可拆到边缘) / 标题栏整卡拖拽
  const handleTabPointerDown = useCallback(
    (e: React.PointerEvent, tabId: string, tabName: string) => {
      if (editingTabId === tabId) return;
      // 关闭按钮/重命名输入框不参与拖拽
      if ((e.target as HTMLElement).closest('button, input')) return;
      dockDrag.begin(e, { kind: 'tab', tab: { kind, tabId, fromCardId: cardId }, label: tabName });
    },
    [editingTabId, kind, cardId]
  );

  const handleTitleBarPointerDown = useCallback(
    (e: React.PointerEvent) => {
      if (editingTabId !== null) return;
      if ((e.target as HTMLElement).closest('button, input')) return;
      const label = tabs.find((t) => t.id === activeTabId)?.name ?? (kind === 'control' ? 'Control' : 'Data');
      dockDrag.begin(e, { kind: 'card', cardId, label });
    },
    [editingTabId, tabs, activeTabId, kind, cardId]
  );

  const handleStartRename = useCallback((tabId: string, currentName: string) => {
    setEditingTabId(tabId);
    setEditName(currentName);
  }, []);

  const handleFinishRename = useCallback(() => {
    if (editingTabId && editName.trim()) {
      renameControlTab(editingTabId, editName.trim());
    }
    setEditingTabId(null);
    setEditName('');
  }, [editingTabId, editName, renameControlTab]);

  // 首次叉掉数据窗口时弹出提示: 关闭窗口不删除节点, 双击画布节点可重新打开
  // (会话级只提示一次, 且遵循全局「上下文提示」开关)
  const maybeShowCloseHint = useCallback(
    (tab: { name: string }) => {
      if (!useSettingsStore.getState().settings.general.showContextualTips) return;
      const st = useOnboardingStore.getState();
      if (st.closeHintShown) return;
      st.markCloseHintShown();
      notify.info(
        t(lang, 'closeHintTitle'),
        t(lang, 'closeHintMessage').replace('{{name}}', tab.name),
        { actions: [{ label: t(lang, 'closeHintGotIt'), run: () => {} }] }
      );
    },
    [lang]
  );

  const tabBarContextMenu = useContextMenu(
    kind === 'control'
      ? [{ id: 'new-tab', label: t(lang, 'newTab'), icon: <Plus />, onClick: () => addControlTab() }]
      : [
          {
            id: 'add-can-tab',
            label: t(lang, 'addCanTab'),
            icon: <Cpu size={14} />,
            disabled: hasCanTab,
            onClick: () => useAppStore.getState().addCanTab(),
          },
          {
            id: 'add-logic-tab',
            label: t(lang, 'addLogicTab'),
            icon: <CircuitBoard size={14} />,
            disabled: hasLogicTab,
            onClick: () => useAppStore.getState().addLogicTab(),
          },
        ]
  );

  const makeTabContextMenu = useCallback(
    (tabId: string, currentName: string) => {
      if (kind === 'control') {
        const allControlTabs = useAppStore.getState().controlTabs;
        const canCloseTab = allControlTabs.length > 1;
        const otherTabs = allControlTabs.filter((tab) => tab.id !== tabId);
        return [
          { id: 'rename', label: t(lang, 'contextMenuRename'), icon: <Type />, onClick: () => handleStartRename(tabId, currentName) },
          { id: 'duplicate', label: t(lang, 'contextMenuDuplicate'), icon: <Copy />, onClick: () => addControlTab(currentName) },
          { kind: 'separator' as const },
          { id: 'close', label: t(lang, 'contextMenuCloseTab'), icon: <Trash2 />, disabled: !canCloseTab, onClick: () => removeControlTab(tabId) },
          {
            id: 'close-others',
            label: t(lang, 'contextMenuCloseOtherTabs'),
            icon: <X />,
            disabled: otherTabs.length === 0,
            onClick: () => otherTabs.forEach((tab) => removeControlTab(tab.id)),
          },
        ];
      }
      const allDataTabs = useAppStore.getState().dataTabs;
      const tab = allDataTabs.find((tb) => tb.id === tabId);
      if (!tab) return [];
      const otherClosable = allDataTabs.filter((tb) => tb.id !== tabId && tb.closable);
      return [
        { id: 'close', label: t(lang, 'contextMenuCloseTab'), icon: <Trash2 size={14} />, disabled: !tab.closable, onClick: () => removeDataTab(tabId) },
        {
          id: 'close-others',
          label: t(lang, 'contextMenuCloseOtherTabs'),
          icon: <X size={14} />,
          disabled: otherClosable.length === 0,
          onClick: () => otherClosable.forEach((tb) => removeDataTab(tb.id)),
        },
      ];
    },
    [kind, lang, addControlTab, removeControlTab, removeDataTab, handleStartRename]
  );

  const closable = (tabId: string) =>
    kind === 'control' ? canClose : (tabs.find((tb) => tb.id === tabId)?.closable ?? false);

  if (!card) return null;

  return (
    <div
      className="module-card dock-card-acrylic relative flex flex-col h-full w-full"
      onMouseDown={() => setFocusedCard(cardId)}
      data-dock-zone="card-edge"
      data-dock-card={cardId}
      data-dock-kind={kind}
    >
      {/* 标题栏 — Tab 条; 空白处拖动 = 整卡移动; 标题栏同时是 Tab 合并投放目标 */}
      <div
        ref={tabBarRef}
        data-tour={kind === 'data' ? 'data-tabs' : undefined}
        className={`relative flex items-center gap-1 bg-bg-panel-header border-b border-border-subtle flex-shrink-0 px-2 py-1 overflow-x-auto transition duration-150 select-none ${
          mergeHover
            ? 'shadow-[inset_0_0_0_1.5px_var(--color-accent)] bg-accent/10'
            : mergeActive
              ? 'bg-accent/5'
              : ''
        }`}
        onContextMenu={tabBarContextMenu}
        onPointerDown={handleTitleBarPointerDown}
        data-dock-zone="merge"
        data-dock-card={cardId}
        data-dock-kind={kind}
        title={t(lang, 'dragToRearrange')}
      >
        <SlidingPill pill={tabPill} />
        {tabs.map((tab) => (
          <div
            key={tab.id}
            data-tab-key={tab.id}
            className={`relative px-2.5 h-7 text-xs cursor-pointer rounded-sm flex items-center gap-1.5 flex-shrink-0 transition-colors duration-150 select-none ${
              tab.id === activeTabId
                ? 'text-text-bright'
                : 'text-text-secondary hover:bg-bg-hover hover:text-text-primary active:bg-accent-active'
            }`}
            onPointerDown={(e) => handleTabPointerDown(e, tab.id, tab.name)}
            onClick={() => {
              if (dockDrag.consumeClick()) return;
              transitionStore(() => setActiveTab(cardId, tab.id));
            }}
            onDoubleClick={() => kind === 'control' && handleStartRename(tab.id, tab.name)}
            onContextMenu={(e) => {
              e.preventDefault();
              e.stopPropagation();
              const items = makeTabContextMenu(tab.id, tab.name);
              if (items.length > 0) showContextMenu(e.clientX, e.clientY, items);
            }}
          >
            {kind === 'data' && <DataTabIcon type={tab.type ?? ''} />}
            {editingTabId === tab.id ? (
              <input
                type="text"
                value={editName}
                onChange={(e) => setEditName(e.target.value)}
                onBlur={handleFinishRename}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') handleFinishRename();
                  if (e.key === 'Escape') setEditingTabId(null);
                }}
                autoFocus
                className="w-[60px] bg-bg-input border border-accent text-text-primary text-xs px-1 py-px rounded-sm"
                onClick={(e) => e.stopPropagation()}
              />
            ) : (
              <span>{tab.name}</span>
            )}
            {closable(tab.id) && (
              <button
                className="w-4 h-4 flex items-center justify-center rounded-sm text-text-secondary hover:bg-bg-hover hover:text-text-primary active:bg-accent-active transition-colors cursor-pointer ml-0.5 p-0 flex-shrink-0"
                onClick={(e) => {
                  e.stopPropagation();
                  if (kind === 'control') removeControlTab(tab.id);
                  else {
                    maybeShowCloseHint(tab);
                    removeDataTab(tab.id);
                  }
                }}
              >
                <X size={10} />
              </button>
            )}
          </div>
        ))}
        {kind === 'control' ? (
          <button
            className="w-6 h-7 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary active:bg-accent-active transition-colors cursor-pointer ml-1 flex-shrink-0"
            onClick={() => addControlTab()}
            title={t(lang, 'newTab')}
          >
            <Plus size={14} />
          </button>
        ) : (
          <>
            <button
              className="w-7 h-7 text-xs cursor-pointer rounded-md flex items-center justify-center flex-shrink-0 text-text-secondary hover:bg-bg-hover hover:text-text-primary active:bg-accent-active transition-colors"
              onClick={() => useAppStore.getState().addCanTab()}
              title={t(lang, 'addCanTab')}
            >
              <Cpu size={12} />
            </button>
            <button
              className="w-7 h-7 text-xs cursor-pointer rounded-md flex items-center justify-center flex-shrink-0 text-text-secondary hover:bg-bg-hover hover:text-text-primary active:bg-accent-active transition-colors"
              onClick={() => useAppStore.getState().addLogicTab()}
              title={t(lang, 'addLogicTab')}
            >
              <CircuitBoard size={12} />
            </button>
          </>
        )}
      </div>

      {/* 内容区 */}
      <div className="flex-1 overflow-hidden relative min-h-0">
        {activeTabId && (
          <AnimatedSwitch switchKey={activeTabId} order={cardTabIds} axis="x" className="h-full w-full">
            {kind === 'control' ? <NodeEditor tabId={activeTabId} /> : <DataTabContent tabId={activeTabId} />}
          </AnimatedSwitch>
        )}
      </div>
    </div>
  );
});
