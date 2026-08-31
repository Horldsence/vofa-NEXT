//! VSCode 风格设置弹窗
//!
//! - 左侧分组导航 (General/Appearance/Editor/Serial/Notifications)
//! - 顶部搜索框 (实时过滤)
//! - 右侧表单 (标题 + 描述 + 控件)
//! - 底部 Reset / Done 按钮
//! - ESC 关闭, 点击遮罩关闭

import { Fragment, useEffect, useMemo, useRef, useState, useActionState } from 'react';
import { activateOnKeyboard } from '../lib/utils/a11y';
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
  Gauge,
  Pencil,
  Download,
  Upload,
  RefreshCw,
  Sparkles,
  ExternalLink,
} from 'lucide-react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { ORCAROUTER_OFFERS_URL, ORCAROUTER_REFERRAL_URL } from '../settings/defaults';
import { getVersion } from '@tauri-apps/api/app';
import { useSettingsStore } from '../store/settingsStore';
import { useAppStore } from '../store/appStore';
import { t } from '../i18n';
import type { Lang } from '../i18n';
import type { AppSettings } from '../settings/defaults';
import { ThemeEditor } from './ThemeEditor';
import { BUILT_IN_THEMES, type ThemeDefinition } from '../settings/theme';
import type { SettingFieldDef} from './settingFields';
import { SETTING_FIELDS } from './settingFields';
import { exportAppToFile, importAppFromFile } from '../lib/tauri/appExport';
import { formatError } from '../lib/tauri/notifications';
import { BackupModal } from './BackupModal';
import { useUpdateStore, type UpdateChannel } from '../store/updateStore';

const CATEGORY_ICONS: Record<keyof AppSettings, React.ReactNode> = {
  general: <SettingsIcon size={16} />,
  appearance: <Palette size={16} />,
  editor: <Sliders size={16} />,
  data: <Database size={16} />,
  serial: <Usb size={16} />,
  notifications: <Bell size={16} />,
  performance: <Gauge size={16} />,
  ai: <Sparkles size={16} />,
};

const CATEGORY_LABEL_KEY: Record<keyof AppSettings, string> = {
  general: 'settingsGeneral',
  appearance: 'settingsAppearance',
  editor: 'settingsEditor',
  data: 'settingsData',
  serial: 'settingsSerial',
  notifications: 'settingsNotifications',
  performance: 'settingsPerformance',
  ai: 'settingsAi',
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
  const [backupModalOpen, setBackupModalOpen] = useState(false);
  const [appVersion, setAppVersion] = useState('');
  /// 更新状态 — 与状态栏/更新弹窗共享 updateStore
  const updateStatus = useUpdateStore((s) => s.status);
  const updateInfo = useUpdateStore((s) => s.updateInfo);
  const updateProgress = useUpdateStore((s) => s.progress);
  const updateError = useUpdateStore((s) => s.error);
  const checkForUpdate = useUpdateStore((s) => s.check);
  const downloadAndInstall = useUpdateStore((s) => s.downloadAndInstall);
  const relaunchApp = useUpdateStore((s) => s.relaunch);
  const setUpdateChannel = useUpdateStore((s) => s.setChannel);
  const updateChannel = useSettingsStore((s) => s.settings.general.updateChannel);
  /// 待确认的通道切换 (非空时显示确认框)
  const [pendingChannel, setPendingChannel] = useState<UpdateChannel | null>(null);

  const searchInputRef = useRef<HTMLInputElement>(null);

  // 读取当前应用版本
  useEffect(() => {
    void getVersion().then(setAppVersion).catch(() => setAppVersion(''));
  }, []);

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

  // Done / Reset Category 提交 action — 包装现有 store action, isPending 禁用按钮
  const [saveState, saveAction, isSaving] = useActionState<{ ok: boolean; error?: string }>(
    () => {
      try {
        close();
        return { ok: true };
      } catch (e) {
        return { ok: false, error: formatError(e) };
      }
    },
    { ok: true }
  );

  const [resetCategoryState, resetCategoryAction, isResettingCategory] = useActionState<{ ok: boolean; error?: string }>(
    () => {
      try {
        resetCategory(activeCategory);
        return { ok: true };
      } catch (e) {
        return { ok: false, error: formatError(e) };
      }
    },
    { ok: true }
  );

  if (!isOpen) return null;

  // 更新按钮行为与状态文案
  const updateBusy = updateStatus === 'checking' || updateStatus === 'downloading';
  const updateLabel =
    updateStatus === 'checking'
      ? t(lang, 'updateChecking')
      : updateStatus === 'available' && updateInfo
        ? `${t(lang, 'updateDownload')} v${updateInfo.version}`
        : updateStatus === 'downloading'
          ? `${t(lang, 'updateDownloading')} ${updateProgress}%`
          : updateStatus === 'ready'
            ? t(lang, 'updateRelaunch')
            : t(lang, 'updateCheck');
  const handleUpdateClick = () => {
    if (updateStatus === 'ready') void relaunchApp();
    else if (updateStatus === 'available') void downloadAndInstall();
    else if (!updateBusy) void checkForUpdate('manual');
  };

  // 更新通道显示值 — 未显式设置时按当前版本推导 (含 '-' 视为预发布)
  const effectiveChannel: UpdateChannel =
    updateChannel ?? (appVersion.includes('-') ? 'beta' : 'stable');

  // 渲染单个控件
  const renderControl = (def: SettingFieldDef) => {
    const category = def.category;
    const value = (settings[category] as Record<string, unknown>)[def.field];

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
            <span className="sr-only">{def.field}</span>
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
    <div
      className="fixed inset-0 bg-bg-overlay z-[9000] flex items-center justify-center animate-[settings-fade-in_0.15s_ease-out]"
      onClick={(event) => { if (event.target === event.currentTarget) close(); }}
      onKeyDown={activateOnKeyboard}
      role="button"
      tabIndex={0}
    >
      <div
        className="w-[820px] max-w-[92vw] h-[600px] max-h-[88vh] bg-bg-sidebar border border-border rounded-lg shadow-modal flex flex-col overflow-hidden animate-[settings-slide-in_0.2s_ease-out]"
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
              className="search-input flex-1 bg-transparent border-none outline-none text-text-primary text-sm font-ui"
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
                  onKeyDown={activateOnKeyboard}
                  role="button"
                  tabIndex={0}
                >
                  {CATEGORY_ICONS[cat]}
                  <span>{t(lang, CATEGORY_LABEL_KEY[cat])}</span>
                </div>
              );
            })}
            <div className="flex-1" />
            <div className="mt-2 border-t border-border px-4 pt-2 pb-1">
              <div className="text-xs font-semibold uppercase tracking-[0.5px] text-text-secondary mb-1">
                {t(lang, 'backupCategory')}
              </div>
              <button
                className="w-full flex items-center gap-2.5 py-1.5 text-text-secondary text-sm cursor-pointer transition-all duration-150 hover:text-text-bright"
                onClick={() => void exportAppToFile()}
                title={t(lang, 'exportConfig')}
              >
                <Download size={14} />
                <span>{t(lang, 'exportConfig')}</span>
              </button>
              <button
                className="w-full flex items-center gap-2.5 py-1.5 text-text-secondary text-sm cursor-pointer transition-all duration-150 hover:text-text-bright"
                onClick={() => void importAppFromFile()}
                title={t(lang, 'importConfig')}
              >
                <Upload size={14} />
                <span>{t(lang, 'importConfig')}</span>
              </button>
              <button
                className="w-full flex items-center gap-2.5 py-1.5 text-text-secondary text-sm cursor-pointer transition-all duration-150 hover:text-text-bright"
                onClick={() => setBackupModalOpen(true)}
                title={t(lang, 'backupCustom')}
              >
                <Database size={14} />
                <span>{t(lang, 'backupCustom')}</span>
              </button>
            </div>
            <div className="mt-2 border-t border-border px-4 pt-2 pb-1">
              <div className="text-xs font-semibold uppercase tracking-[0.5px] text-text-secondary mb-1">
                {t(lang, 'updateCategory')}
              </div>
              {appVersion && (
                <div className="text-xs text-text-secondary pb-1">
                  {t(lang, 'updateCurrentVersion')}: v{appVersion}
                </div>
              )}
              <button
                className={`w-full flex items-center gap-2.5 py-1.5 text-text-secondary text-sm transition-all duration-150 ${
                  updateBusy ? 'opacity-60 cursor-wait' : 'cursor-pointer hover:text-text-bright'
                }`}
                onClick={handleUpdateClick}
                disabled={updateBusy}
                title={updateLabel}
              >
                <RefreshCw size={14} className={updateBusy ? 'animate-spin' : ''} />
                <span>{updateLabel}</span>
              </button>
              <div className="flex items-center justify-between gap-2 py-1.5">
                <span className="text-xs text-text-secondary" title={t(lang, 'updateChannelDesc')}>
                  {t(lang, 'updateChannel')}
                </span>
                <select
                  className="px-2 py-0.5 bg-bg-input text-text-primary border border-border rounded text-xs focus:outline-none focus:border-accent transition-colors cursor-pointer"
                  value={effectiveChannel}
                  onChange={(e) => {
                    const next = e.target.value as UpdateChannel;
                    if (next !== effectiveChannel) setPendingChannel(next);
                  }}
                >
                  <option value="stable">{t(lang, 'updateChannelStable')}</option>
                  <option value="beta">{t(lang, 'updateChannelBeta')}</option>
                </select>
              </div>
              {updateStatus === 'downloading' && (
                <div className="h-1 rounded-full bg-bg-hover overflow-hidden mb-1">
                  <div
                    className="h-full bg-accent transition-all duration-200"
                    style={{ width: `${updateProgress}%` }}
                  />
                </div>
              )}
              {updateStatus === 'up-to-date' && (
                <div className="text-xs text-text-secondary pb-1">{t(lang, 'updateUpToDate')}</div>
              )}
              {updateStatus === 'ready' && (
                <div className="text-xs text-text-secondary pb-1">{t(lang, 'updateReady')}</div>
              )}
              {updateStatus === 'error' && updateError && (
                <div className="text-xs text-red-400 pb-1 break-all">
                  {t(lang, 'updateError')}: {updateError}
                </div>
              )}
            </div>
            <div
              className="flex items-center gap-2.5 px-4 py-2 text-text-secondary text-sm cursor-pointer transition-all duration-150 border-l-2 border-transparent hover:bg-bg-hover hover:text-text-primary"
              onClick={reset}
              onKeyDown={activateOnKeyboard}
              role="button"
              tabIndex={0}
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
                    <Fragment key={`${def.category}-${def.field}`}>
                      <div className="flex items-start justify-between gap-6 py-2.5 border-b border-border last:border-b-0">
                        <div className="flex-1 min-w-0">
                          <div className="text-sm text-text-primary mb-0.5">{t(lang, def.labelKey)}</div>
                          <div className="text-xs text-text-secondary leading-relaxed">{t(lang, def.descKey)}</div>
                        </div>
                        <div className="flex-shrink-0 min-w-[200px] flex items-center justify-end">{renderControl(def)}</div>
                      </div>
                      {/* OrcaRouter 重点提示: 命名空间模型名 + 推广链接获取 API Key + 免费模型入口 */}
                      {cat === 'ai' && def.field === 'apiKey' && settings.ai.adapter === 'orcarouter' && (
                        <div className="my-2 px-3 py-2 rounded border border-accent/25 bg-accent/5 space-y-1.5">
                          <div className="flex items-center gap-2">
                            <span className="flex-1 min-w-0 text-xs text-text-secondary leading-relaxed">
                              {t(lang, 'settingAiOrcaHint')}
                            </span>
                            <button
                              className="flex-shrink-0 px-2 py-1 rounded bg-accent text-accent-foreground text-xs flex items-center gap-1"
                              onClick={() => void openUrl(ORCAROUTER_REFERRAL_URL)}
                            >
                              <ExternalLink size={11} />
                              {t(lang, 'settingAiOrcaGetKey')}
                            </button>
                          </div>
                          <div className="flex items-center gap-1.5 text-xs text-text-secondary leading-relaxed">
                            <Sparkles size={11} className="flex-shrink-0 text-accent" />
                            <span className="flex-shrink-0">{t(lang, 'settingAiOrcaFreeModels')}</span>
                            <button
                              className="text-accent hover:text-accent-hover hover:underline truncate text-left"
                              onClick={() => void openUrl(ORCAROUTER_OFFERS_URL)}
                              title={ORCAROUTER_OFFERS_URL}
                            >
                              {ORCAROUTER_OFFERS_URL}
                            </button>
                          </div>
                        </div>
                      )}
                    </Fragment>
                  ))}
                </div>
              );
            })}
            {!searchQuery.trim() && (
              <form action={resetCategoryAction} className="py-4">
                <button
                  type="submit"
                  className="px-3 py-1.5 bg-transparent text-text-secondary border-none rounded cursor-pointer text-sm text-center transition-colors hover:bg-bg-hover hover:text-text-primary inline-flex items-center gap-1.5 disabled:opacity-50 disabled:cursor-default"
                  disabled={isResettingCategory}
                >
                  <RotateCcw size={12} />
                  <span>{t(lang, 'settingsResetCategory')}</span>
                </button>
                {resetCategoryState.error && (
                  <div className="text-xs text-red-400 break-all">{resetCategoryState.error}</div>
                )}
              </form>
            )}
          </div>
        </div>

        {/* 底部 — 完成按钮 */}
        <div className="flex items-center px-4 py-2.5 border-t border-border bg-bg-panel-header flex-shrink-0">
          <div className="flex-1" />
          <form action={saveAction}>
            <button
              type="submit"
              className="bg-bg-button text-text-inverse border-none py-1.5 px-4 text-sm font-ui cursor-pointer rounded inline-flex items-center gap-1.5 transition-colors hover:bg-bg-button-hover disabled:opacity-50 disabled:cursor-default"
              disabled={isSaving}
            >
              <Check size={14} />
              <span>{t(lang, 'settingsDone')}</span>
            </button>
            {saveState.error && (
              <div className="text-xs text-red-400 break-all">{saveState.error}</div>
            )}
          </form>
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
        cssThemes={settings.appearance.customCssThemes}
        onCssThemesChange={(themes) =>
          (update as (c: keyof AppSettings, f: string, v: unknown) => void)(
            'appearance',
            'customCssThemes',
            themes
          )
        }
        activeCssThemeId={String(settings.appearance.cssTheme)}
        onActiveCssThemeChange={(id) =>
          (update as (c: keyof AppSettings, f: string, v: unknown) => void)(
            'appearance',
            'cssTheme',
            id
          )
        }
      />
      <BackupModal isOpen={backupModalOpen} onClose={() => setBackupModalOpen(false)} />
      {/* 更新通道切换确认 — select 为受控值, 取消即自动回原值 */}
      {pendingChannel && (
        <div
          className="fixed inset-0 bg-bg-overlay z-[9500] flex items-center justify-center animate-[settings-fade-in_0.15s_ease-out]"
          onClick={(e) => {
            // 阻止冒泡到设置弹窗遮罩 (避免整个设置弹窗被关闭)
            e.stopPropagation();
            setPendingChannel(null);
          }}
          onKeyDown={activateOnKeyboard}
          role="button"
          tabIndex={0}
        >
          <div
            className="w-[360px] max-w-[90vw] bg-bg-sidebar border border-border rounded-lg shadow-modal p-5 flex flex-col gap-3 animate-[settings-slide-in_0.2s_ease-out]"
            role="dialog"
            aria-modal="true"
          >
            <div className="text-sm font-semibold text-text-bright">
              {t(lang, 'updateChannelConfirmTitle')}
            </div>
            <div className="text-xs text-text-secondary leading-relaxed">
              {t(lang, pendingChannel === 'beta' ? 'updateChannelToBetaMsg' : 'updateChannelToStableMsg')}
            </div>
            <div className="flex justify-end gap-2">
              <button
                className="bg-transparent text-text-primary border border-border px-2.5 py-1 text-xs cursor-pointer rounded transition-all hover:bg-bg-hover hover:border-accent hover:text-text-bright"
                onClick={() => setPendingChannel(null)}
              >
                {t(lang, 'updateCancel')}
              </button>
              <button
                className="px-3 py-1 bg-bg-button text-text-inverse border-none rounded cursor-pointer text-xs transition-colors hover:bg-bg-button-hover"
                onClick={() => {
                  setUpdateChannel(pendingChannel);
                  setPendingChannel(null);
                }}
              >
                {t(lang, 'updateConfirm')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
