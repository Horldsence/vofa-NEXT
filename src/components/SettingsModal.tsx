//! VSCode 风格设置弹窗
//!
//! - 左侧分组导航 (General/Appearance/Editor/Serial/Notifications)
//! - 顶部搜索框 (实时过滤)
//! - 右侧表单 (标题 + 描述 + 控件)
//! - 底部 Reset / Done 按钮
//! - ESC 关闭, 点击遮罩关闭

import { useEffect, useMemo, useRef } from 'react';
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
} from 'lucide-react';
import { useSettingsStore } from '../store/settingsStore';
import { useAppStore } from '../store/appStore';
import { t } from '../i18n';
import type { Lang } from '../i18n';
import type { AppSettings } from '../settings/defaults';

const CATEGORY_ICONS: Record<keyof AppSettings, React.ReactNode> = {
  general: <SettingsIcon size={16} />,
  appearance: <Palette size={16} />,
  editor: <Sliders size={16} />,
  serial: <Usb size={16} />,
  notifications: <Bell size={16} />,
};

const CATEGORY_LABEL_KEY: Record<keyof AppSettings, string> = {
  general: 'settingsGeneral',
  appearance: 'settingsAppearance',
  editor: 'settingsEditor',
  serial: 'settingsSerial',
  notifications: 'settingsNotifications',
};

type ControlType =
  | { kind: 'select'; options: { value: string | number; label: string }[] }
  | { kind: 'toggle' }
  | { kind: 'number'; min?: number; max?: number; step?: number }
  | { kind: 'text' };

interface SettingFieldDef {
  category: keyof AppSettings;
  field: string;
  labelKey: string;
  descKey: string;
  control: ControlType;
  /// 用于搜索的关键词 (除了 label/desc)
  keywords?: string[];
}

/// 所有设置项的元数据 — 按分类顺序渲染
const SETTING_FIELDS: SettingFieldDef[] = [
  // General
  {
    category: 'general',
    field: 'language',
    labelKey: 'settingLanguage',
    descKey: 'settingLanguageDesc',
    control: {
      kind: 'select',
      options: [
        { value: 'zh', label: '中文' },
        { value: 'en', label: 'English' },
      ],
    },
  },
  {
    category: 'general',
    field: 'autoConnectOnStart',
    labelKey: 'settingAutoConnectOnStart',
    descKey: 'settingAutoConnectOnStartDesc',
    control: { kind: 'toggle' },
  },
  {
    category: 'general',
    field: 'confirmBeforeQuit',
    labelKey: 'settingConfirmBeforeQuit',
    descKey: 'settingConfirmBeforeQuitDesc',
    control: { kind: 'toggle' },
  },
  // Appearance
  {
    category: 'appearance',
    field: 'theme',
    labelKey: 'settingTheme',
    descKey: 'settingThemeDesc',
    control: {
      kind: 'select',
      options: [
        { value: 'dark', label: 'Dark' },
        { value: 'light', label: 'Light (TODO)' },
      ],
    },
  },
  {
    category: 'appearance',
    field: 'uiFontFamily',
    labelKey: 'settingUiFontFamily',
    descKey: 'settingUiFontFamilyDesc',
    control: { kind: 'text' },
  },
  {
    category: 'appearance',
    field: 'uiFontSize',
    labelKey: 'settingUiFontSize',
    descKey: 'settingUiFontSizeDesc',
    control: { kind: 'number', min: 9, max: 24, step: 1 },
  },
  {
    category: 'appearance',
    field: 'monoFontFamily',
    labelKey: 'settingMonoFontFamily',
    descKey: 'settingMonoFontFamilyDesc',
    control: { kind: 'text' },
  },
  {
    category: 'appearance',
    field: 'monoFontSize',
    labelKey: 'settingMonoFontSize',
    descKey: 'settingMonoFontSizeDesc',
    control: { kind: 'number', min: 9, max: 24, step: 1 },
  },
  {
    category: 'appearance',
    field: 'statusBarVisible',
    labelKey: 'settingStatusBarVisible',
    descKey: 'settingStatusBarVisibleDesc',
    control: { kind: 'toggle' },
  },
  {
    category: 'appearance',
    field: 'activityBarVisible',
    labelKey: 'settingActivityBarVisible',
    descKey: 'settingActivityBarVisibleDesc',
    control: { kind: 'toggle' },
  },
  // Editor
  {
    category: 'editor',
    field: 'waveformMaxPoints',
    labelKey: 'settingWaveformMaxPoints',
    descKey: 'settingWaveformMaxPointsDesc',
    control: { kind: 'number', min: 1000, max: 1000000, step: 1000 },
  },
  {
    category: 'editor',
    field: 'waveformFps',
    labelKey: 'settingWaveformFps',
    descKey: 'settingWaveformFpsDesc',
    control: { kind: 'number', min: 5, max: 120, step: 1 },
  },
  {
    category: 'editor',
    field: 'scopeDefaultTimeBase',
    labelKey: 'settingScopeDefaultTimeBase',
    descKey: 'settingScopeDefaultTimeBaseDesc',
    control: { kind: 'number', min: 0.0001, max: 10, step: 0.0001 },
  },
  {
    category: 'editor',
    field: 'scopeDefaultVPerDiv',
    labelKey: 'settingScopeDefaultVPerDiv',
    descKey: 'settingScopeDefaultVPerDivDesc',
    control: { kind: 'number', min: 0.001, max: 100, step: 0.001 },
  },
  {
    category: 'editor',
    field: 'gridVisible',
    labelKey: 'settingGridVisible',
    descKey: 'settingGridVisibleDesc',
    control: { kind: 'toggle' },
  },
  // Serial
  {
    category: 'serial',
    field: 'defaultBaudRate',
    labelKey: 'settingDefaultBaudRate',
    descKey: 'settingDefaultBaudRateDesc',
    control: {
      kind: 'select',
      options: [
        { value: 9600, label: '9600' },
        { value: 19200, label: '19200' },
        { value: 38400, label: '38400' },
        { value: 57600, label: '57600' },
        { value: 115200, label: '115200' },
        { value: 230400, label: '230400' },
        { value: 460800, label: '460800' },
        { value: 921600, label: '921600' },
      ],
    },
  },
  {
    category: 'serial',
    field: 'defaultDataBits',
    labelKey: 'settingDefaultDataBits',
    descKey: 'settingDefaultDataBitsDesc',
    control: {
      kind: 'select',
      options: [
        { value: 7, label: '7' },
        { value: 8, label: '8' },
      ],
    },
  },
  {
    category: 'serial',
    field: 'defaultParity',
    labelKey: 'settingDefaultParity',
    descKey: 'settingDefaultParityDesc',
    control: {
      kind: 'select',
      options: [
        { value: 'none', label: 'None' },
        { value: 'odd', label: 'Odd' },
        { value: 'even', label: 'Even' },
      ],
    },
  },
  {
    category: 'serial',
    field: 'defaultStopBits',
    labelKey: 'settingDefaultStopBits',
    descKey: 'settingDefaultStopBitsDesc',
    control: {
      kind: 'select',
      options: [
        { value: 'one', label: '1' },
        { value: 'two', label: '2' },
      ],
    },
  },
  {
    category: 'serial',
    field: 'defaultFlowControl',
    labelKey: 'settingDefaultFlowControl',
    descKey: 'settingDefaultFlowControlDesc',
    control: {
      kind: 'select',
      options: [
        { value: 'none', label: 'None' },
        { value: 'software', label: 'Software' },
        { value: 'hardware', label: 'Hardware' },
      ],
    },
  },
  // Notifications
  {
    category: 'notifications',
    field: 'enabled',
    labelKey: 'settingNotifEnabled',
    descKey: 'settingNotifEnabledDesc',
    control: { kind: 'toggle' },
  },
  {
    category: 'notifications',
    field: 'duration',
    labelKey: 'settingNotifDuration',
    descKey: 'settingNotifDurationDesc',
    control: { kind: 'number', min: 0, max: 60000, step: 500 },
  },
  {
    category: 'notifications',
    field: 'showOnConnect',
    labelKey: 'settingNotifShowOnConnect',
    descKey: 'settingNotifShowOnConnectDesc',
    control: { kind: 'toggle' },
  },
  {
    category: 'notifications',
    field: 'showOnDisconnect',
    labelKey: 'settingNotifShowOnDisconnect',
    descKey: 'settingNotifShowOnDisconnectDesc',
    control: { kind: 'toggle' },
  },
  {
    category: 'notifications',
    field: 'showOnError',
    labelKey: 'settingNotifShowOnError',
    descKey: 'settingNotifShowOnErrorDesc',
    control: { kind: 'toggle' },
  },
];

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
            className="settings-select"
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
            className="settings-input settings-input-number"
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
            className="settings-input"
            value={String(value)}
            onChange={(e) => handleChange(e.target.value)}
          />
        );
    }
  };

  return (
    <div className="settings-overlay" onClick={close}>
      <div
        className="settings-modal"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
      >
        {/* 顶部 — 标题 + 搜索框 + 关闭 */}
        <div className="settings-header">
          <div className="settings-title-row">
            <Type size={16} />
            <span className="settings-title-text">{t(lang, 'settingsTitle')}</span>
          </div>
          <div className="settings-search-row">
            <Search size={14} className="settings-search-icon" />
            <input
              ref={searchInputRef}
              type="text"
              className="settings-search-input"
              placeholder={t(lang, 'settingsSearch')}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
            />
          </div>
          <button className="settings-close-btn" onClick={close} title={t(lang, 'settingsClose')}>
            <X size={16} />
          </button>
        </div>

        {/* 主体 — 左侧分类 + 右侧表单 */}
        <div className="settings-body">
          <div className="settings-sidebar">
            {(Object.keys(CATEGORY_LABEL_KEY) as (keyof AppSettings)[]).map((cat) => {
              const isActive =
                !searchQuery.trim() && activeCategory === cat;
              return (
                <div
                  key={cat}
                  className={`settings-category-item${isActive ? ' active' : ''}`}
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
            <div style={{ flex: 1 }} />
            <div
              className="settings-category-item"
              onClick={reset}
              title={t(lang, 'settingsReset')}
            >
              <RotateCcw size={14} />
              <span>{t(lang, 'settingsReset')}</span>
            </div>
          </div>

          <div className="settings-content">
            {(Object.keys(groupedFields) as (keyof AppSettings)[]).map((cat) => {
              const fields = groupedFields[cat]!;
              // 搜索模式下显示分类标题; 非搜索模式只显示当前分类
              if (!searchQuery.trim() && cat !== activeCategory) return null;
              return (
                <div key={cat} className="settings-group">
                  {searchQuery.trim() && (
                    <div className="settings-group-title">
                      {t(lang, CATEGORY_LABEL_KEY[cat])}
                    </div>
                  )}
                  {fields.map((def) => (
                    <div key={`${def.category}-${def.field}`} className="settings-row">
                      <div className="settings-row-label">
                        <div className="settings-row-title">{t(lang, def.labelKey)}</div>
                        <div className="settings-row-desc">{t(lang, def.descKey)}</div>
                      </div>
                      <div className="settings-row-control">{renderControl(def)}</div>
                    </div>
                  ))}
                </div>
              );
            })}
            {!searchQuery.trim() && (
              <div className="settings-group-actions">
                <button
                  className="btn-secondary"
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
        <div className="settings-footer">
          <div style={{ flex: 1 }} />
          <button className="btn-primary" onClick={close}>
            <Check size={14} />
            <span>{t(lang, 'settingsDone')}</span>
          </button>
        </div>
      </div>
    </div>
  );
}
