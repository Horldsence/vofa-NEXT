import { useState, useCallback, useEffect } from 'react';
import { Plus, X, Type, Trash2, Copy, Cpu, CircuitBoard } from 'lucide-react';
import { useAppStore } from '../../store/appStore';
import { useDockStore } from '../../store/dockStore';
import { useSlidingPill, SlidingPill } from '../ui/SlidingPill';
import { AnimatedSwitch } from '../ui/AnimatedSwitch';
import { useSnapDrop } from '../ui/SnapDropOverlay';
import { NodeEditor } from './NodeEditor';
import { DataTabContent, DataTabIcon } from './DataTabContent';
import { useContextMenu, showContextMenu } from '../../lib/hooks/useContextMenu';
import { transitionStore } from '../../lib/utils/transitionStore';
import { t } from '../../i18n';

/// 通用 Dock 卡片框架 — 标题栏 (Tab 条 + 滑动指示器) + 内容区 + 吸附投放层
/// 交互:
/// - 拖动单个 Tab 到本卡片标题栏 → 合并为本卡片的一个 Tab
/// - 拖动单个 Tab 到卡片边缘 → 拆分为独立面板
/// - 拖动标题栏空白处 → 整卡移动到其他卡片边缘
export function DockCardFrame({ cardId }: { cardId: string }) {
  const lang = useAppStore((s) => s.lang);
  const card = useDockStore((s) => s.cards[cardId]);
  const controlTabs = useAppStore((s) => s.controlTabs);
  const dataTabs = useAppStore((s) => s.dataTabs);
  const addControlTab = useAppStore((s) => s.addControlTab);
  const removeControlTab = useAppStore((s) => s.removeControlTab);
  const renameControlTab = useAppStore((s) => s.renameControlTab);
  const removeDataTab = useAppStore((s) => s.removeDataTab);

  const draggingTab = useDockStore((s) => s.draggingTab);
  const draggingCardId = useDockStore((s) => s.draggingCardId);
  const setDraggingTab = useDockStore((s) => s.setDraggingTab);
  const setDraggingCard = useDockStore((s) => s.setDraggingCard);
  const setActiveTab = useDockStore((s) => s.setActiveTab);
  const setFocusedCard = useDockStore((s) => s.setFocusedCard);
  const moveTabToCard = useDockStore((s) => s.moveTabToCard);
  const dropOnCardEdge = useDockStore((s) => s.dropOnCardEdge);
  const setDropTarget = useDockStore((s) => s.setDropTarget);

  const [editingTabId, setEditingTabId] = useState<string | null>(null);
  const [editName, setEditName] = useState('');
  const [mergeHover, setMergeHover] = useState(false);

  // 卡片可能因树折叠在渲染间隙被移除 — hook 需无条件调用, 用兜底值
  const kind = card?.kind ?? 'data';
  const cardTabIds = card?.tabIds ?? [];
  const activeTabId = card ? (card.activeTabId ?? card.tabIds[0] ?? null) : null;

  // 本卡片承载的 Tab 元数据 (保持 appStore 中的顺序)
  const tabs =
    kind === 'control'
      ? controlTabs.filter((tab) => cardTabIds.includes(tab.id))
      : dataTabs.filter((tab) => cardTabIds.includes(tab.id));

  // Tab 滑动指示器
  const { containerRef: tabBarRef, pill: tabPill } = useSlidingPill(activeTabId);

  // 边缘吸附 — Tab 拆分 (允许跨 kind; 多 Tab 卡片可拆自身) 或整卡移动
  const snapActive =
    (draggingTab !== null && (draggingTab.fromCardId !== cardId || cardTabIds.length > 1)) ||
    (draggingCardId !== null && draggingCardId !== cardId);
  const { edge: snapEdge, handlers: snapHandlers } = useSnapDrop(snapActive, (edge) => {
    dropOnCardEdge(cardId, edge);
  });

  // 悬停边缘上报到 dockStore → DockLayout 渲染全局预览 (区域与实际落点一致)
  const dropTarget = useDockStore((s) => s.dropTarget);
  useEffect(() => {
    if (snapEdge) {
      setDropTarget({ cardId, edge: snapEdge });
    } else if (dropTarget?.cardId === cardId) {
      setDropTarget(null);
    }
  }, [snapEdge, cardId, dropTarget, setDropTarget]);

  // 标题栏为 Tab 合并投放目标 (仅同 kind 的跨卡片 Tab 拖拽)
  const mergeActive = draggingTab !== null && draggingTab.kind === kind && draggingTab.fromCardId !== cardId;

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

  const tabBarContextMenu = useContextMenu(
    kind === 'control'
      ? [{ id: 'new-tab', label: t(lang, 'newTab'), icon: <Plus />, onClick: () => addControlTab() }]
      : [
          {
            id: 'add-can-tab',
            label: t(lang, 'addCanTab'),
            icon: <Cpu size={14} />,
            disabled: dataTabs.some((tab) => tab.type === 'can'),
            onClick: () => useAppStore.getState().addCanTab(),
          },
          {
            id: 'add-logic-tab',
            label: t(lang, 'addLogicTab'),
            icon: <CircuitBoard size={14} />,
            disabled: dataTabs.some((tab) => tab.type === 'logic'),
            onClick: () => useAppStore.getState().addLogicTab(),
          },
        ]
  );

  const makeTabContextMenu = useCallback(
    (tabId: string, currentName: string) => {
      if (kind === 'control') {
        const canClose = controlTabs.length > 1;
        const otherTabs = controlTabs.filter((tab) => tab.id !== tabId);
        return [
          { id: 'rename', label: t(lang, 'contextMenuRename'), icon: <Type />, onClick: () => handleStartRename(tabId, currentName) },
          { id: 'duplicate', label: t(lang, 'contextMenuDuplicate'), icon: <Copy />, onClick: () => addControlTab(currentName) },
          { kind: 'separator' as const },
          { id: 'close', label: t(lang, 'contextMenuCloseTab'), icon: <Trash2 />, disabled: !canClose, onClick: () => removeControlTab(tabId) },
          {
            id: 'close-others',
            label: t(lang, 'contextMenuCloseOtherTabs'),
            icon: <X />,
            disabled: otherTabs.length === 0,
            onClick: () => otherTabs.forEach((tab) => removeControlTab(tab.id)),
          },
        ];
      }
      const tab = dataTabs.find((tb) => tb.id === tabId);
      if (!tab) return [];
      const otherClosable = dataTabs.filter((tb) => tb.id !== tabId && tb.closable);
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
    [kind, controlTabs, dataTabs, lang, addControlTab, removeControlTab, removeDataTab, handleStartRename]
  );

  const closable = (tabId: string) =>
    kind === 'control' ? controlTabs.length > 1 : (dataTabs.find((tb) => tb.id === tabId)?.closable ?? false);

  if (!card) return null;

  return (
    <div
      className="module-card relative flex flex-col bg-bg-editor h-full w-full"
      onMouseDown={() => setFocusedCard(cardId)}
      {...snapHandlers}
    >
      {/* 标题栏 — Tab 条; 空白处拖动 = 整卡移动 */}
      <div
        ref={tabBarRef}
        data-tour={kind === 'data' ? 'data-tabs' : undefined}
        className={`relative flex items-center gap-1 bg-bg-panel-header border-b border-border flex-shrink-0 p-1 overflow-x-auto ${
          mergeHover ? 'shadow-[inset_0_0_0_1.5px_var(--color-accent)]' : ''
        }`}
        onContextMenu={tabBarContextMenu}
        draggable={editingTabId === null}
        onDragStart={(e) => {
          e.dataTransfer.setData('text/plain', `card:${cardId}`);
          e.dataTransfer.effectAllowed = 'move';
          setDraggingCard(cardId);
        }}
        onDragEnd={() => {
          setDraggingCard(null);
          setDropTarget(null);
        }}
        onDragOver={(e) => {
          if (!mergeActive) return;
          e.preventDefault();
          e.stopPropagation();
          e.dataTransfer.dropEffect = 'move';
          setMergeHover(true);
        }}
        onDragLeave={() => setMergeHover(false)}
        onDrop={(e) => {
          if (!mergeActive) return;
          e.preventDefault();
          e.stopPropagation();
          setMergeHover(false);
          moveTabToCard(cardId);
        }}
        title={t(lang, 'dragToRearrange')}
      >
        <SlidingPill pill={tabPill} />
        {tabs.map((tab) => (
          <div
            key={tab.id}
            data-tab-key={tab.id}
            className={`relative px-2.5 h-7 text-xs cursor-pointer rounded-md flex items-center gap-1.5 flex-shrink-0 transition-colors duration-150 ${
              tab.id === activeTabId
                ? 'text-text-bright'
                : 'text-text-secondary hover:bg-bg-hover hover:text-text-primary'
            }`}
            draggable={editingTabId !== tab.id}
            onDragStart={(e) => {
              e.stopPropagation();
              e.dataTransfer.setData('text/plain', `tab:${tab.id}`);
              e.dataTransfer.effectAllowed = 'move';
              setDraggingTab({ kind, tabId: tab.id, fromCardId: cardId });
            }}
            onDragEnd={() => {
              setDraggingTab(null);
              setDropTarget(null);
            }}
            onClick={() => transitionStore(() => setActiveTab(cardId, tab.id))}
            onDoubleClick={() => kind === 'control' && handleStartRename(tab.id, tab.name)}
            onContextMenu={(e) => {
              e.preventDefault();
              e.stopPropagation();
              const items = makeTabContextMenu(tab.id, tab.name);
              if (items.length > 0) showContextMenu(e.clientX, e.clientY, items);
            }}
          >
            {kind === 'data' && <DataTabIcon type={(tab as { type?: string }).type ?? ''} />}
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
                className="w-4 h-4 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer ml-0.5 p-0"
                onClick={(e) => {
                  e.stopPropagation();
                  if (kind === 'control') removeControlTab(tab.id);
                  else removeDataTab(tab.id);
                }}
              >
                <X size={10} />
              </button>
            )}
          </div>
        ))}
        {kind === 'control' ? (
          <button
            className="w-6 h-7 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer ml-1"
            onClick={() => addControlTab()}
            title={t(lang, 'newTab')}
          >
            <Plus size={14} />
          </button>
        ) : (
          <>
            <button
              className="w-7 h-7 text-xs cursor-pointer rounded-md flex items-center justify-center flex-shrink-0 text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors"
              onClick={() => useAppStore.getState().addCanTab()}
              title={t(lang, 'addCanTab')}
            >
              <Cpu size={12} />
            </button>
            <button
              className="w-7 h-7 text-xs cursor-pointer rounded-md flex items-center justify-center flex-shrink-0 text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors"
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
}
