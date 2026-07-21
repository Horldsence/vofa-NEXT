//! 主题编辑器弹窗
//!
//! - 左侧主题列表 (内置 + 自定义)
//! - 右侧颜色 token 网格
//! - 支持新建/复制/重命名/删除自定义主题

import { useEffect, useMemo, useRef, useState } from 'react';
import {
  X,
  Plus,
  Copy,
  Trash2,
  Check,
  RotateCcw,
  Palette,
  Download,
  Upload,
} from 'lucide-react';
import { useAppStore } from '../store/appStore';
import { t } from '../i18n';
import {
  BUILT_IN_THEMES,
  DARK_THEME,
  LIGHT_THEME,
  TOKEN_GROUPS,
  TOKEN_LABELS,
  THEME_TOKENS,
  applyTheme,
  applyThemeToken,
  createCustomTheme,
  getTokenGroup,
  type ThemeDefinition,
  type ThemeToken,
} from '../settings/theme';

interface ThemeEditorProps {
  isOpen: boolean;
  onClose: () => void;
  themes: ThemeDefinition[];
  onThemesChange: (themes: ThemeDefinition[]) => void;
  activeThemeId: string;
  onActiveThemeChange: (id: string) => void;
}

const GROUP_LABELS: Record<(typeof TOKEN_GROUPS)[keyof typeof TOKEN_GROUPS], string> = {
  background: '背景',
  border: '边框',
  text: '文字',
  accent: '强调色',
};

const GROUP_ORDER: (keyof typeof GROUP_LABELS)[] = ['background', 'text', 'border', 'accent'];

/// 将颜色值转换为可用于 <input type="color"> 的 hex (无法解析则返回 null)
function toColorInputValue(value: string): string | null {
  const trimmed = value.trim();
  if (/^#[0-9a-fA-F]{6}$/.test(trimmed)) return trimmed;
  // 简单解析 rgb/rgba
  const rgbMatch = trimmed.match(/rgba?\((\d+),\s*(\d+),\s*(\d+)/);
  if (rgbMatch) {
    const r = Number(rgbMatch[1]).toString(16).padStart(2, '0');
    const g = Number(rgbMatch[2]).toString(16).padStart(2, '0');
    const b = Number(rgbMatch[3]).toString(16).padStart(2, '0');
    return `#${r}${g}${b}`;
  }
  return null;
}

/// 颜色选择 + 文本输入组合控件
function ColorField({
  token,
  value,
  onChange,
  disabled,
}: {
  token: ThemeToken;
  value: string;
  onChange: (v: string) => void;
  disabled?: boolean;
}) {
  const colorValue = useMemo(() => toColorInputValue(value), [value]);

  return (
    <div className="flex items-center gap-2 py-1.5">
      <div
        className="w-6 h-6 rounded border border-border flex-shrink-0"
        style={{ background: value }}
      />
      <div className="flex-1 min-w-0">
        <div className="text-xs text-text-primary mb-0.5">{TOKEN_LABELS[token]}</div>
        <div className="flex items-center gap-1.5">
          <input
            type="color"
            value={colorValue ?? '#000000'}
            disabled={disabled || !colorValue}
            onChange={(e) => onChange(e.target.value)}
            className="w-7 h-7 p-0 border-0 rounded cursor-pointer bg-transparent disabled:opacity-50"
            title={token}
          />
          <input
            type="text"
            value={value}
            disabled={disabled}
            onChange={(e) => onChange(e.target.value)}
            className="flex-1 min-w-0 px-1.5 py-0.5 bg-bg-input text-text-primary border border-border rounded text-[11px] font-mono focus:outline-none focus:border-accent disabled:opacity-50"
          />
        </div>
      </div>
    </div>
  );
}

export function ThemeEditor({
  isOpen,
  onClose,
  themes,
  onThemesChange,
  activeThemeId,
  onActiveThemeChange,
}: ThemeEditorProps) {
  const lang = useAppStore((s) => s.lang);
  const [selectedThemeId, setSelectedThemeId] = useState(activeThemeId);
  const [renameValue, setRenameValue] = useState('');
  const [renaming, setRenaming] = useState(false);
  const renameInputRef = useRef<HTMLInputElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  // 打开时同步选中当前激活主题
  useEffect(() => {
    if (isOpen) setSelectedThemeId(activeThemeId);
  }, [isOpen, activeThemeId]);

  // ESC 关闭
  useEffect(() => {
    if (!isOpen) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [isOpen, onClose]);

  const allThemes = useMemo(
    () => [...BUILT_IN_THEMES, ...themes],
    [themes]
  );

  const selectedTheme = useMemo(
    () => allThemes.find((t) => t.id === selectedThemeId) ?? DARK_THEME,
    [allThemes, selectedThemeId]
  );

  const isBuiltIn = selectedTheme.isBuiltIn;

  const groupedTokens = useMemo(() => {
    const groups: Record<string, ThemeToken[]> = {};
    for (const token of THEME_TOKENS) {
      const g = getTokenGroup(token);
      (groups[g] ??= []).push(token);
    }
    return groups;
  }, []);

  const updateSelectedTheme = (patch: Partial<ThemeDefinition> | ((prev: ThemeDefinition) => ThemeDefinition)) => {
    if (isBuiltIn) return;
    const next = typeof patch === 'function' ? patch(selectedTheme) : { ...selectedTheme, ...patch };
    const newThemes = themes.map((t) => (t.id === next.id ? next : t));
    onThemesChange(newThemes);
    // 实时预览: 如果编辑的就是当前激活主题, 直接应用 token
    if (selectedThemeId === activeThemeId) {
      applyTheme(next);
    }
  };

  const handleTokenChange = (token: ThemeToken, value: string) => {
    updateSelectedTheme((prev) => ({
      ...prev,
      tokens: { ...prev.tokens, [token]: value },
    }));
    if (selectedThemeId === activeThemeId) {
      applyThemeToken(token, value);
    }
  };

  const handleAddTheme = () => {
    const base = allThemes.find((t) => t.id === selectedThemeId) ?? DARK_THEME;
    const newTheme = createCustomTheme(`${base.name} Copy`, base);
    onThemesChange([...themes, newTheme]);
    setSelectedThemeId(newTheme.id);
    onActiveThemeChange(newTheme.id);
    applyTheme(newTheme);
  };

  const handleDuplicate = () => {
    const newTheme = createCustomTheme(`${selectedTheme.name} Copy`, selectedTheme);
    onThemesChange([...themes, newTheme]);
    setSelectedThemeId(newTheme.id);
  };

  const handleDelete = () => {
    if (isBuiltIn) return;
    const newThemes = themes.filter((t) => t.id !== selectedTheme.id);
    onThemesChange(newThemes);
    // 若删除的是当前激活主题, 回退到 dark
    if (selectedTheme.id === activeThemeId) {
      onActiveThemeChange(DARK_THEME.id);
      applyTheme(DARK_THEME);
    }
    setSelectedThemeId(DARK_THEME.id);
  };

  const startRename = () => {
    if (isBuiltIn) return;
    setRenameValue(selectedTheme.name);
    setRenaming(true);
    setTimeout(() => renameInputRef.current?.focus(), 50);
  };

  const commitRename = () => {
    const name = renameValue.trim();
    if (name && !isBuiltIn) {
      updateSelectedTheme({ name });
    }
    setRenaming(false);
  };

  const handleReset = () => {
    if (isBuiltIn) return;
    const base = selectedThemeId === LIGHT_THEME.id ? LIGHT_THEME : DARK_THEME;
    updateSelectedTheme({ tokens: { ...base.tokens } });
    if (selectedThemeId === activeThemeId) {
      applyTheme({ ...selectedTheme, tokens: { ...base.tokens } });
    }
  };

  const handleExport = () => {
    const data = JSON.stringify(selectedTheme, null, 2);
    const blob = new Blob([data], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${selectedTheme.name.replace(/\s+/g, '_')}.theme.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const handleImportClick = () => fileInputRef.current?.click();

  const handleImportFile = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      try {
        const imported = JSON.parse(String(reader.result)) as ThemeDefinition;
        if (!imported.tokens || !imported.name) return;
        const newTheme: ThemeDefinition = {
          ...imported,
          id: `custom-${Date.now()}`,
          isBuiltIn: false,
        };
        onThemesChange([...themes, newTheme]);
        setSelectedThemeId(newTheme.id);
        onActiveThemeChange(newTheme.id);
        applyTheme(newTheme);
      } catch {
        // ignore invalid import
      }
    };
    reader.readAsText(file);
    e.target.value = '';
  };

  if (!isOpen) return null;

  return (
    <div
      className="fixed inset-0 bg-bg-overlay z-[9100] flex items-center justify-center animate-[settings-fade-in_0.15s_ease-out]"
      onClick={onClose}
    >
      <div
        className="w-[880px] max-w-[94vw] h-[640px] max-h-[90vh] bg-bg-sidebar border border-border rounded-lg shadow-modal flex flex-col overflow-hidden animate-[settings-slide-in_0.2s_ease-out]"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
      >
        {/* 顶部 */}
        <div className="flex items-center gap-3 px-4 py-3 border-b border-border bg-bg-panel-header flex-shrink-0">
          <div className="flex items-center gap-1.5 text-text-primary text-base font-semibold flex-shrink-0">
            <Palette size={16} />
            <span>{t(lang, 'themeEditor')}</span>
          </div>
          <div className="flex-1" />
          <button
            className="w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer bg-transparent border-none"
            onClick={onClose}
            title={t(lang, 'themeEditorClose')}
          >
            <X size={16} />
          </button>
        </div>

        {/* 主体 */}
        <div className="flex-1 flex min-h-0">
          {/* 左侧主题列表 */}
          <div className="w-56 bg-bg-sidebar border-r border-border flex flex-col flex-shrink-0">
            <div className="flex items-center justify-between px-3 py-2 border-b border-border">
              <span className="text-xs font-semibold text-text-secondary uppercase tracking-[0.5px]">
                {t(lang, 'themeList')}
              </span>
              <button
                className="w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer bg-transparent border-none"
                onClick={handleAddTheme}
                title={t(lang, 'themeAdd')}
              >
                <Plus size={14} />
              </button>
            </div>
            <div className="flex-1 overflow-y-auto py-1">
              {allThemes.map((theme) => {
                const active = theme.id === selectedThemeId;
                return (
                  <div
                    key={theme.id}
                    className={`flex items-center gap-2 px-3 py-2 text-sm cursor-pointer transition-colors ${
                      active ? 'bg-bg-active text-text-primary' : 'text-text-secondary hover:bg-bg-hover hover:text-text-primary'
                    }`}
                    onClick={() => setSelectedThemeId(theme.id)}
                  >
                    <div
                      className="w-3 h-3 rounded-full border border-border flex-shrink-0"
                      style={{ background: theme.tokens.bgEditor }}
                    />
                    <span className="flex-1 truncate">{theme.name}</span>
                    {theme.id === activeThemeId && (
                      <Check size={12} className="text-accent flex-shrink-0" />
                    )}
                  </div>
                );
              })}
            </div>
          </div>

          {/* 右侧编辑器 */}
          <div className="flex-1 flex flex-col min-w-0 bg-bg-editor">
            {/* 工具栏 */}
            <div className="flex items-center gap-2 px-4 py-2.5 border-b border-border flex-shrink-0">
              {renaming && !isBuiltIn ? (
                <input
                  ref={renameInputRef}
                  type="text"
                  value={renameValue}
                  onChange={(e) => setRenameValue(e.target.value)}
                  onBlur={commitRename}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') commitRename();
                    if (e.key === 'Escape') setRenaming(false);
                  }}
                  className="px-2 py-1 bg-bg-input text-text-primary border border-border rounded text-sm font-semibold focus:outline-none focus:border-accent"
                />
              ) : (
                <button
                  className="text-sm font-semibold text-text-primary hover:bg-bg-hover px-2 py-1 rounded cursor-pointer bg-transparent border-none"
                  onClick={startRename}
                  disabled={isBuiltIn}
                  title={isBuiltIn ? undefined : t(lang, 'themeRename')}
                >
                  {selectedTheme.name}
                </button>
              )}
              <div className="flex-1" />
              {!isBuiltIn && (
                <>
                  <button
                    className="px-2 py-1 text-xs text-text-secondary hover:bg-bg-hover hover:text-text-primary rounded inline-flex items-center gap-1 cursor-pointer bg-transparent border-none"
                    onClick={handleDuplicate}
                    title={t(lang, 'themeDuplicate')}
                  >
                    <Copy size={12} />
                    <span>{t(lang, 'themeDuplicate')}</span>
                  </button>
                  <button
                    className="px-2 py-1 text-xs text-text-secondary hover:bg-bg-danger-hover hover:text-text-inverse rounded inline-flex items-center gap-1 cursor-pointer bg-transparent border-none"
                    onClick={handleDelete}
                    title={t(lang, 'themeDelete')}
                  >
                    <Trash2 size={12} />
                    <span>{t(lang, 'themeDelete')}</span>
                  </button>
                </>
              )}
              <div className="w-px h-4 bg-border mx-1" />
              <button
                className="px-2 py-1 text-xs text-text-secondary hover:bg-bg-hover hover:text-text-primary rounded inline-flex items-center gap-1 cursor-pointer bg-transparent border-none"
                onClick={handleImportClick}
                title={t(lang, 'themeImport')}
              >
                <Upload size={12} />
              </button>
              <input
                ref={fileInputRef}
                type="file"
                accept=".json"
                className="hidden"
                onChange={handleImportFile}
              />
              <button
                className="px-2 py-1 text-xs text-text-secondary hover:bg-bg-hover hover:text-text-primary rounded inline-flex items-center gap-1 cursor-pointer bg-transparent border-none"
                onClick={handleExport}
                title={t(lang, 'themeExport')}
              >
                <Download size={12} />
              </button>
              {!isBuiltIn && (
                <button
                  className="px-2 py-1 text-xs text-text-secondary hover:bg-bg-hover hover:text-text-primary rounded inline-flex items-center gap-1 cursor-pointer bg-transparent border-none"
                  onClick={handleReset}
                  title={t(lang, 'themeReset')}
                >
                  <RotateCcw size={12} />
                  <span>{t(lang, 'themeReset')}</span>
                </button>
              )}
            </div>

            {/* 只读提示 */}
            {isBuiltIn && (
              <div className="px-4 py-2 bg-bg-active text-text-primary text-xs flex-shrink-0">
                {t(lang, 'themeBuiltInReadOnly')}
              </div>
            )}

            {/* Token 网格 */}
            <div className="flex-1 overflow-y-auto px-4 py-3">
              {GROUP_ORDER.map((group) => {
                const tokens = groupedTokens[group];
                if (!tokens?.length) return null;
                return (
                  <div key={group} className="mb-5">
                    <div className="text-xs font-semibold uppercase tracking-[0.5px] text-text-secondary pb-2 mb-2 border-b border-border">
                      {GROUP_LABELS[group]}
                    </div>
                    <div className="grid grid-cols-2 gap-x-4 gap-y-1">
                      {tokens.map((token) => (
                        <ColorField
                          key={token}
                          token={token}
                          value={selectedTheme.tokens[token]}
                          onChange={(v) => handleTokenChange(token, v)}
                          disabled={isBuiltIn}
                        />
                      ))}
                    </div>
                  </div>
                );
              })}
            </div>

            {/* 底部操作 */}
            <div className="flex items-center justify-between px-4 py-3 border-t border-border bg-bg-panel-header flex-shrink-0">
              <div className="flex items-center gap-3">
                <div
                  className="w-8 h-8 rounded border border-border"
                  style={{
                    background: `linear-gradient(135deg, ${selectedTheme.tokens.bgEditor} 50%, ${selectedTheme.tokens.bgSidebar} 50%)`,
                  }}
                />
                <div className="text-xs text-text-secondary">
                  {selectedThemeId === activeThemeId
                    ? t(lang, 'themeActivePreview')
                    : t(lang, 'themeClickToPreview')}
                </div>
              </div>
              <div className="flex items-center gap-2">
                {selectedThemeId !== activeThemeId && !isBuiltIn && (
                  <button
                    className="px-3 py-1.5 bg-bg-button text-text-inverse border-none rounded text-sm cursor-pointer hover:bg-bg-button-hover"
                    onClick={() => {
                      onActiveThemeChange(selectedTheme.id);
                      applyTheme(selectedTheme);
                    }}
                  >
                    {t(lang, 'themeActivate')}
                  </button>
                )}
                <button
                  className="px-3 py-1.5 bg-transparent text-text-primary border border-border rounded text-sm cursor-pointer hover:bg-bg-hover"
                  onClick={onClose}
                >
                  {t(lang, 'themeEditorClose')}
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
