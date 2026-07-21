//! 应用设置 schema 与默认值
//!
//! 与 settingsStore.ts 中的 AppSettings 接口对应
//! 通过 tauri-plugin-store 持久化到 app config dir 的 settings.json

import type { Lang } from '../i18n';
import type { ThemeDefinition } from './theme';

/// 应用设置根 schema
export interface AppSettings {
  general: {
    language: Lang;
    autoConnectOnStart: boolean;
    confirmBeforeQuit: boolean;
    showOnboarding: boolean;
    showContextualTips: boolean;
    debug: boolean;
  };
  appearance: {
    theme: string;
    customThemes: ThemeDefinition[];
    uiFontFamily: string;
    uiFontSize: number;
    monoFontFamily: string;
    monoFontSize: number;
    statusBarVisible: boolean;
    activityBarVisible: boolean;
    acrylicBackground: boolean;
    /// 毛玻璃面板透明度 0.1-1.0, 0.6 为基准观感
    acrylicOpacity: number;
    /// 毛玻璃模糊半径 (仅 macOS 生效, 0 表示系统默认)
    acrylicBlurRadius: number;
  };
  editor: {
    waveformFps: number;
    scopeDefaultTimeBase: number;
    scopeDefaultVPerDiv: number;
    gridVisible: boolean;
  };
  data: {
    waveformBufferPoints: number;
    rawDataBufferBytes: number;
    canBufferFrames: number;
    logicBufferSamples: number;
  };
  serial: {
    defaultBaudRate: number;
    defaultDataBits: 7 | 8;
    defaultParity: 'none' | 'odd' | 'even';
    defaultStopBits: 'one' | 'two';
    defaultFlowControl: 'none' | 'software' | 'hardware';
  };
  notifications: {
    enabled: boolean;
    duration: number;
    showOnConnect: boolean;
    showOnDisconnect: boolean;
    showOnError: boolean;
  };
}

/// 默认设置 — 与项目当前行为保持一致
export const DEFAULT_SETTINGS: AppSettings = {
  general: {
    language: 'zh',
    autoConnectOnStart: false,
    confirmBeforeQuit: true,
    showOnboarding: true,
    showContextualTips: true,
    debug: false,
  },
  appearance: {
    theme: 'dark',
    customThemes: [],
    uiFontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif",
    uiFontSize: 13,
    monoFontFamily: "'Cascadia Code', 'Fira Code', 'SF Mono', Menlo, monospace",
    monoFontSize: 12,
    statusBarVisible: true,
    activityBarVisible: true,
    acrylicBackground: false,
    acrylicOpacity: 0.6,
    acrylicBlurRadius: 0,
  },
  editor: {
    waveformFps: 30,
    scopeDefaultTimeBase: 100e-3,
    scopeDefaultVPerDiv: 1,
    gridVisible: true,
  },
  data: {
    waveformBufferPoints: 100_000,
    rawDataBufferBytes: 1_048_576,
    canBufferFrames: 100_000,
    logicBufferSamples: 20_000,
  },
  serial: {
    defaultBaudRate: 115200,
    defaultDataBits: 8,
    defaultParity: 'none',
    defaultStopBits: 'one',
    defaultFlowControl: 'none',
  },
  notifications: {
    enabled: true,
    duration: 5000,
    showOnConnect: true,
    showOnDisconnect: false,
    showOnError: true,
  },
};

/// 设置分类元数据 — 用于 SettingsModal 渲染左侧导航
export interface SettingCategoryMeta {
  key: keyof AppSettings;
  icon: string; // lucide-react icon name
}

export const SETTING_CATEGORIES: SettingCategoryMeta[] = [
  { key: 'general', icon: 'Settings' },
  { key: 'appearance', icon: 'Palette' },
  { key: 'editor', icon: 'Sliders' },
  { key: 'data', icon: 'Database' },
  { key: 'serial', icon: 'Usb' },
  { key: 'notifications', icon: 'Bell' },
];

/// 浅合并: 用任意子路径更新设置 (path 例如 'appearance.uiFontSize')
export function deepMergeSettings(
  base: AppSettings,
  patch: Partial<AppSettings>
): AppSettings {
  const result: AppSettings = JSON.parse(JSON.stringify(base));
  for (const k of Object.keys(patch) as (keyof AppSettings)[]) {
    const v = patch[k];
    if (v && typeof v === 'object') {
      // 浅合并子对象
      (result[k] as Record<string, unknown>) = { ...(result[k] as Record<string, unknown>), ...(v as Record<string, unknown>) };
    } else if (v !== undefined) {
      // 顶层标量
      (result[k] as unknown) = v;
    }
  }
  return result;
}
