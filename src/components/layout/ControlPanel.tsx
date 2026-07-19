import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { useContextMenu, showContextMenu } from '../../lib/useContextMenu';
import { Plus, X, Type, Trash2, Copy } from 'lucide-react';
import { NodeEditor } from './NodeEditor';
import { useState, useCallback } from 'react';

/// 控件区 — 多标签页 + Blender 风格节点编辑器
/// 从侧边栏拖拽控件到画布，连接线表示数据通道
export function ControlPanel() {
  const lang = useAppStore((s) => s.lang);
  const controlTabs = useAppStore((s) => s.controlTabs);
  const activeControlTabId = useAppStore((s) => s.activeControlTabId);
  const addControlTab = useAppStore((s) => s.addControlTab);
  const removeControlTab = useAppStore((s) => s.removeControlTab);
  const setActiveControlTab = useAppStore((s) => s.setActiveControlTab);
  const renameControlTab = useAppStore((s) => s.renameControlTab);

  const [editingTabId, setEditingTabId] = useState<string | null>(null);
  const [editName, setEditName] = useState('');

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

  const tabBarContextMenu = useContextMenu([
    {
      id: 'new-tab',
      label: t(lang, 'newTab'),
      icon: <Plus />,
      onClick: () => addControlTab(),
    },
  ]);

  const makeTabContextMenu = useCallback(
    (tabId: string, currentName: string) => {
      const canClose = controlTabs.length > 1;
      const otherTabs = controlTabs.filter((t) => t.id !== tabId);
      return [
        {
          id: 'rename',
          label: t(lang, 'contextMenuRename'),
          icon: <Type />,
          onClick: () => handleStartRename(tabId, currentName),
        },
        {
          id: 'duplicate',
          label: t(lang, 'contextMenuDuplicate'),
          icon: <Copy />,
          onClick: () => addControlTab(currentName),
        },
        { kind: 'separator' as const },
        {
          id: 'close',
          label: t(lang, 'contextMenuCloseTab'),
          icon: <Trash2 />,
          disabled: !canClose,
          onClick: () => removeControlTab(tabId),
        },
        {
          id: 'close-others',
          label: t(lang, 'contextMenuCloseOtherTabs'),
          icon: <X />,
          disabled: otherTabs.length === 0,
          onClick: () => {
            otherTabs.forEach((t) => removeControlTab(t.id));
          },
        },
      ];
    },
    [addControlTab, controlTabs, handleStartRename, lang, removeControlTab]
  );

  return (
    <div className="flex flex-col bg-bg-editor overflow-hidden h-full w-full">
      <div className="flex bg-bg-panel-header border-b border-border flex-shrink-0" onContextMenu={tabBarContextMenu}>
        {controlTabs.map((tab) => (
          <div
            key={tab.id}
            className={`px-3 h-7 text-xs cursor-pointer border-r border-border flex items-center gap-1 hover:bg-bg-hover transition-colors ${tab.id === activeControlTabId ? 'text-text-bright bg-bg-editor border-t-2 border-t-accent' : 'text-text-secondary'}`}
            onClick={() => setActiveControlTab(tab.id)}
            onDoubleClick={() => handleStartRename(tab.id, tab.name)}
            onContextMenu={(e) => {
              e.preventDefault();
              e.stopPropagation();
              const items = makeTabContextMenu(tab.id, tab.name);
              if (items.length > 0) {
                showContextMenu(e.clientX, e.clientY, items);
              }
            }}
          >
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
            {controlTabs.length > 1 && (
              <button
                className="w-4 h-4 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer ml-0.5 p-0"
                onClick={(e) => {
                  e.stopPropagation();
                  removeControlTab(tab.id);
                }}
              >
                <X size={10} />
              </button>
            )}
          </div>
        ))}
        <button
          className="w-6 h-7 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer ml-1"
          onClick={() => addControlTab()}
          title={t(lang, 'newTab')}
        >
          <Plus size={14} />
        </button>
      </div>
      <div className="flex-1 overflow-hidden relative min-h-0">
        <NodeEditor tabId={activeControlTabId} />
      </div>
    </div>
  );
}
