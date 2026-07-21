//! VSCode 风格设置弹窗
//!
//! - 左侧分组导航 (General/Appearance/Editor/Serial/Notifications)
//! - 顶部搜索框 (实时过滤)
//! - 右侧表单 (标题 + 描述 + 控件)
//! - 底部 Reset / Done 按钮
//! - ESC 关闭, 点击遮罩关闭

import { useEffect, useMemo, useRef, useState } from 'react';
import {
  X,
  Search,
  RotateCcw,
  Check,
  Settings as SettingsIcon,
  Palette,
  Sliders,
  Usb,
  Bell,
  Type,
  Database,
  Pencil,
} from 'lucide-react';
import { useSettingsStore } from '../store/settingsStore';
import { useAppStore } from '../store/appStore';
import { t } from '../i18n';
import type { Lang } from '../i18n';
import type { AppSettings } from '../settings/defaults';
import { ThemeEditor } from './ThemeEditor';
import { BUILT_IN_THEMES, type ThemeDefinition } from '../settings/theme';
import { SettingFieldDef, SETTING_FIELDS } from './settingFields';

const CATEGORY_ICONS: Record<keyof AppSettings, React.ReactNode> = {
  general: <SettingsIcon size={16} />,
  appearance: <Palette size={16} />,
  editor: <Sliders size={16} />,
  data: <Database size={16} />,
  serial: <Usb size={16} />,
  notifications: <Bell size={16} />,
};

const CATEGORY_LABEL_KEY: Record<keyof AppSettings, string> = {
  general: 'settingsGeneral',
  appearance: 'settingsAppearance',
  editor: 'settingsEditor',
  data: 'settingsData',
  serial: 'settingsSerial',
  notifications: 'settingsNotifications',
};

export function SettingsModal() {
  const lang = useAppStore((s) => s.lang);
  const setLang = useAppStore((s) => s.setLang);
  const isOpen = useSettingsStore((s) => s.isOpen);
  const close = useSettingsStore((s) => s.close);
  const settings = useSettingsStore((s) => s.settings);
  const activeCategory = useSettingsStore((s) => s.activeCategory);
  const searchQuery = useSettingsStore((s) => s.searchQuery);
  const setActiveCategory = useSettingsStore((s) => s.setActiveCategory);
  const setSearchQuery = useSettingsStore((s) => s.setSearchQuery);
  const update = useSettingsStore((s) => s.update);
  const reset = useSettingsStore((s) => s.reset);
  const resetCategory = useSettingsStore((s) => s.resetCategory);
  const [themeEditorOpen, setThemeEditorOpen] = useState(false);

  const searchInputRef = useRef<HTMLInputElement>(null);

  // ESC 关闭 + 自动聚焦搜索框
  useEffect(() => {
    if (!isOpen) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') close();
    };
    window.addEventListener('keydown', handler);
    // 延迟聚焦避免与打开动画冲突
    const t = setTimeout(() => searchInputRef.current?.focus(), 50);
    return () => {
      window.removeEventListener('keydown', handler);
      clearTimeout(t);
    };
  }, [isOpen, close]);

  // 搜索过滤
  const filteredFields = useMemo(() => {
    if (!searchQuery.trim()) return SETTING_FIELDS;
    const q = searchQuery.toLowerCase();
    return SETTING_FIELDS.filter((f) => {
      const label = t(lang, f.labelKey).toLowerCase();
      const desc = t(lang, f.descKey).toLowerCase();
      const category = t(lang, CATEGORY_LABEL_KEY[f.category]).toLowerCase();
      return (
        label.includes(q) ||
        desc.includes(q) ||
        category.includes(q) ||
        f.field.toLowerCase().includes(q) ||
        (f.keywords?.some((k) => k.toLowerCase().includes(q)) ?? false)
      );
    });
  }, [lang, searchQuery]);

  // 按分类分组
  const groupedFields = useMemo(() => {
    const groups: Partial<Record<keyof AppSettings, SettingFieldDef[]>> = {};
    for (const f of filteredFields) {
      (groups[f.category] ??= []).push(f);
    }
    return groups;
  }, [filteredFields]);

  if (!isOpen) return null;

  // 渲染单个控件
  const renderControl = (def: SettingFieldDef) => {
    const category = def.category;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const value = (settings[category] as any)[def.field];

    const handleChange = (v: unknown) => {
      // 设置项的 category+field 组合来自 SETTING_FIELDS 静态表, 类型保证安全
      // 但 TypeScript 无法静态推断, 此处用 type assertion
      (update as (c: keyof AppSettings, f: string, v: unknown) => void)(
        category,
        def.field,
        v
      );
      // 语言切换同步到 appStore
      if (category === 'general' && def.field === 'language') {
        setLang(v as Lang);
      }
    };

    const ctrl = def.control;
    switch (ctrl.kind) {
      case 'toggle':
        return (
          <label className="settings-toggle">
            <input
              type="checkbox"
              checked={Boolean(value)}
              onChange={(e) => handleChange(e.target.checked)}
            />
            <span className="settings-toggle-slider" />
          </label>
        );
      case 'select':
        return (
          <select
            className="w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded text-sm focus:outline-none focus:border-accent transition-colors cursor-pointer min-w-[140px]"
            value={String(value)}
            onChange={(e) => {
              const opt = ctrl.options.find((o) => String(o.value) === e.target.value);
              if (opt) handleChange(opt.value);
            }}
          >
            {ctrl.options.map((o) => (
              <option key={String(o.value)} value={String(o.value)}>
                {o.label}
              </option>
            ))}
          </select>
        );
      case 'number':
        return (
          <input
            type="number"
            className="w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded text-sm font-ui focus:outline-none focus:border-accent transition-colors w-[120px]"
            value={value as number}
            min={ctrl.min}
            max={ctrl.max}
            step={ctrl.step}
            onChange={(e) => {
              const n = Number(e.target.value);
              if (!Number.isNaN(n)) handleChange(n);
            }}
          />
        );
      case 'text':
        return (
          <input
            type="text"
            className="w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded text-sm font-ui focus:outline-none focus:border-accent transition-colors"
            value={String(value)}
            onChange={(e) => handleChange(e.target.value)}
          />
        );
      case 'theme': {
        const themeOptions = [
          ...BUILT_IN_THEMES.map((t) => ({ value: t.id, label: t.name })),
          ...settings.appearance.customThemes.map((t: ThemeDefinition) => ({
            value: t.id,
            label: t.name,
          })),
        ];
        return (
          <div className="flex items-center gap-2 min-w-[220px]">
            <select
              className="flex-1 px-2 py-1 bg-bg-input text-text-primary border border-border rounded text-sm focus:outline-none focus:border-accent transition-colors cursor-pointer"
              value={String(value)}
              onChange={(e) => {
                const opt = themeOptions.find((o) => o.value === e.target.value);
                if (opt) handleChange(opt.value);
              }}
            >
              {themeOptions.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </select>
            <button
              className="px-2 py-1 bg-bg-input text-text-primary border border-border rounded text-sm hover:bg-bg-hover hover:text-text-bright transition-colors cursor-pointer inline-flex items-center gap-1"
              onClick={() => setThemeEditorOpen(true)}
              title={t(lang, 'themeEdit')}
            >
              <Pencil size={12} />
              <span>{t(lang, 'themeEdit')}</span>
            </button>
          </div>
        );
      }
    }
  };

  return (
    <div className="fixed inset-0 bg-bg-overlay z-[9000] flex items-center justify-center animate-[settings-fade-in_0.15s_ease-out]" onClick={close}>
      <div
        className="w-[820px] max-w-[92vw] h-[600px] max-h-[88vh] bg-bg-sidebar border border-border rounded-lg shadow-modal flex flex-col overflow-hidden animate-[settings-slide-in_0.2s_ease-out]"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
      >
        {/* 顶部 — 标题 + 搜索框 + 关闭 */}
        <div className="flex items-center gap-3 px-4 py-3 border-b border-border bg-bg-panel-header flex-shrink-0">
          <div className="flex items-center gap-1.5 text-text-bright text-base font-semibold flex-shrink-0">
            <Type size={16} />
            <span>{t(lang, 'settingsTitle')}</span>
          </div>
          <div className="flex-1 flex items-center gap-1.5 bg-bg-input border border-border rounded px-2 py-1 focus-within:border-accent transition-colors">
            <Search size={14} className="text-text-secondary flex-shrink-0" />
            <input
              ref={searchInputRef}
              type="text"
              className="flex-1 bg-transparent border-none outline-none text-text-primary text-sm font-ui"
              placeholder={t(lang, 'settingsSearch')}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
            />
          </div>
          <button className="w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer" onClick={close} title={t(lang, 'settingsClose')}>
            <X size={16} />
          </button>
        </div>

        {/* 主体 — 左侧分类 + 右侧表单 */}
        <div className="flex-1 flex min-h-0">
          <div className="w-50 bg-bg-sidebar border-r border-border py-2 flex flex-col flex-shrink-0 overflow-y-auto">
            {(Object.keys(CATEGORY_LABEL_KEY) as (keyof AppSettings)[]).map((cat) => {
              const isActive =
                !searchQuery.trim() && activeCategory === cat;
              return (
                <div
                  key={cat}
                  className={`flex items-center gap-2.5 px-4 py-2 text-text-secondary text-sm cursor-pointer transition-all duration-150 border-l-2 border-transparent hover:bg-bg-hover hover:text-text-primary ${isActive ? 'text-text-bright bg-bg-hover border-l-accent' : ''}`}
                  onClick={() => {
                    setActiveCategory(cat);
                    setSearchQuery('');
                  }}
                >
                  {CATEGORY_ICONS[cat]}
                  <span>{t(lang, CATEGORY_LABEL_KEY[cat])}</span>
                </div>
              );
            })}
            <div className="flex-1" />
            <div
              className="flex items-center gap-2.5 px-4 py-2 text-text-secondary text-sm cursor-pointer transition-all duration-150 border-l-2 border-transparent hover:bg-bg-hover hover:text-text-primary"
              onClick={reset}
              title={t(lang, 'settingsReset')}
            >
              <RotateCcw size={14} />
              <span>{t(lang, 'settingsReset')}</span>
            </div>
          </div>

          <div className="flex-1 overflow-y-auto px-6 py-4 bg-bg-editor">
            {(Object.keys(groupedFields) as (keyof AppSettings)[]).map((cat) => {
              const fields = groupedFields[cat]!;
              // 搜索模式下显示分类标题; 非搜索模式只显示当前分类
              if (!searchQuery.trim() && cat !== activeCategory) return null;
              return (
                <div key={cat} className="mb-6">
                  {searchQuery.trim() && (
                    <div className="text-xs font-semibold uppercase tracking-[0.5px] text-text-secondary pb-2 mb-3 border-b border-border">
                      {t(lang, CATEGORY_LABEL_KEY[cat])}
                    </div>
                  )}
                  {fields.map((def) => (
                    <div key={`${def.category}-${def.field}`} className="flex items-start justify-between gap-6 py-2.5 border-b border-border last:border-b-0">
                      <div className="flex-1 min-w-0">
                        <div className="text-sm text-text-primary mb-0.5">{t(lang, def.labelKey)}</div>
                        <div className="text-xs text-text-secondary leading-relaxed">{t(lang, def.descKey)}</div>
                      </div>
                      <div className="flex-shrink-0 min-w-[200px] flex items-center justify-end">{renderControl(def)}</div>
                    </div>
                  ))}
                </div>
              );
            })}
            {!searchQuery.trim() && (
              <div className="py-4 flex gap-2">
                <button
                  className="px-3 py-1.5 bg-transparent text-text-secondary border-none rounded cursor-pointer text-sm text-center transition-colors hover:bg-bg-hover hover:text-text-primary inline-flex items-center gap-1.5"
                  onClick={() => resetCategory(activeCategory)}
                >
                  <RotateCcw size={12} />
                  <span>{t(lang, 'settingsResetCategory')}</span>
                </button>
              </div>
            )}
          </div>
        </div>

        {/* 底部 — 完成按钮 */}
        <div className="flex items-center px-4 py-2.5 border-t border-border bg-bg-panel-header flex-shrink-0">
          <div className="flex-1" />
          <button className="bg-bg-button text-text-inverse border-none py-1.5 px-4 text-sm font-ui cursor-pointer rounded inline-flex items-center gap-1.5 transition-colors hover:bg-bg-button-hover" onClick={close}>
            <Check size={14} />
            <span>{t(lang, 'settingsDone')}</span>
          </button>
        </div>
      </div>
      <ThemeEditor
        isOpen={themeEditorOpen}
        onClose={() => setThemeEditorOpen(false)}
        themes={settings.appearance.customThemes}
        onThemesChange={(themes) =>
          (update as (c: keyof AppSettings, f: string, v: unknown) => void)(
            'appearance',
            'customThemes',
            themes
          )
        }
        activeThemeId={String(settings.appearance.theme)}
        onActiveThemeChange={(id) =>
          (update as (c: keyof AppSettings, f: string, v: unknown) => void)(
            'appearance',
            'theme',
            id
          )
        }
      />
    </div>
  );
}
