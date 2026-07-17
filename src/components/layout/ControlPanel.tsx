import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { Plus, X } from 'lucide-react';
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

  return (
    <div className="panel">
      <div className="tabs">
        {controlTabs.map((tab) => (
          <div
            key={tab.id}
            className={`tab ${tab.id === activeControlTabId ? 'active' : ''}`}
            onClick={() => setActiveControlTab(tab.id)}
            onDoubleClick={() => handleStartRename(tab.id, tab.name)}
            style={{ cursor: 'pointer' }}
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
                style={{
                  width: 60,
                  background: 'var(--bg-input)',
                  border: '1px solid var(--accent)',
                  color: 'var(--text-primary)',
                  fontSize: 11,
                  padding: '1px 4px',
                }}
                onClick={(e) => e.stopPropagation()}
              />
            ) : (
              <span>{tab.name}</span>
            )}
            {controlTabs.length > 1 && (
              <button
                className="btn-icon"
                style={{ marginLeft: 2, padding: 0, width: 16, height: 16 }}
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
          className="btn-icon"
          onClick={() => addControlTab()}
          title={t(lang, 'newTab')}
          style={{ marginLeft: 4 }}
        >
          <Plus size={14} />
        </button>
      </div>
      <div className="panel-content">
        <NodeEditor tabId={activeControlTabId} />
      </div>
    </div>
  );
}