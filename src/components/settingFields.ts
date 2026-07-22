//! 设置项元数据定义 — 控制类型、字段接口、所有设置项列表
//!
//! 从 SettingsModal.tsx 提取，用于保持文件体积可控

import type { AppSettings } from '../settings/defaults';

export type ControlType =
  | { kind: 'select'; options: { value: string | number; label: string }[] }
  | { kind: 'toggle' }
  | { kind: 'number'; min?: number; max?: number; step?: number }
  | { kind: 'text' }
  | { kind: 'theme' };

export interface SettingFieldDef {
  category: keyof AppSettings;
  field: string;
  labelKey: string;
  descKey: string;
  control: ControlType;
  /// 用于搜索的关键词 (除了 label/desc)
  keywords?: string[];
}

/// 所有设置项的元数据 — 按分类顺序渲染
export const SETTING_FIELDS: SettingFieldDef[] = [
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
  {
    category: 'general',
    field: 'showOnboarding',
    labelKey: 'settingShowOnboarding',
    descKey: 'settingShowOnboardingDesc',
    control: { kind: 'toggle' },
  },
  {
    category: 'general',
    field: 'showContextualTips',
    labelKey: 'settingShowContextualTips',
    descKey: 'settingShowContextualTipsDesc',
    control: { kind: 'toggle' },
  },
  {
    category: 'general',
    field: 'debug',
    labelKey: 'settingDebug',
    descKey: 'settingDebugDesc',
    control: { kind: 'toggle' },
  },
  // Appearance
  {
    category: 'appearance',
    field: 'theme',
    labelKey: 'settingTheme',
    descKey: 'settingThemeDesc',
    control: { kind: 'theme' },
  },
  {
    category: 'appearance',
    field: 'acrylicBackground',
    labelKey: 'settingAcrylicBackground',
    descKey: 'settingAcrylicBackgroundDesc',
    control: { kind: 'toggle' },
  },
  {
    category: 'appearance',
    field: 'acrylicOpacity',
    labelKey: 'settingAcrylicOpacity',
    descKey: 'settingAcrylicOpacityDesc',
    control: { kind: 'number', min: 0.1, max: 1, step: 0.05 },
  },
  {
    category: 'appearance',
    field: 'acrylicBlurRadius',
    labelKey: 'settingAcrylicBlurRadius',
    descKey: 'settingAcrylicBlurRadiusDesc',
    control: { kind: 'number', min: 0, max: 100, step: 1 },
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
  {
    category: 'editor',
    field: 'cursorReadoutVisible',
    labelKey: 'settingCursorReadoutVisible',
    descKey: 'settingCursorReadoutVisibleDesc',
    control: { kind: 'toggle' },
    keywords: ['cursor', 'readout', '光标', '读数'],
  },
  {
    category: 'editor',
    field: 'cursorSnap',
    labelKey: 'settingCursorSnap',
    descKey: 'settingCursorSnapDesc',
    control: { kind: 'toggle' },
    keywords: ['snap', 'cursor', '吸附', '光标'],
  },
  {
    category: 'editor',
    field: 'crosshairVisible',
    labelKey: 'settingCrosshairVisible',
    descKey: 'settingCrosshairVisibleDesc',
    control: { kind: 'toggle' },
    keywords: ['crosshair', '十字线'],
  },
  {
    category: 'editor',
    field: 'hoverPointsVisible',
    labelKey: 'settingHoverPointsVisible',
    descKey: 'settingHoverPointsVisibleDesc',
    control: { kind: 'toggle' },
    keywords: ['hover', 'points', '采样点', 'uplot'],
  },
  // Data
  {
    category: 'data',
    field: 'waveformBufferPoints',
    labelKey: 'settingWaveformBufferPoints',
    descKey: 'settingWaveformBufferPointsDesc',
    control: { kind: 'number', min: 1000, max: 1_000_000, step: 1000 },
  },
  {
    category: 'data',
    field: 'rawDataBufferBytes',
    labelKey: 'settingRawDataBufferBytes',
    descKey: 'settingRawDataBufferBytesDesc',
    control: { kind: 'number', min: 65536, max: 16_777_216, step: 65536 },
  },
  {
    category: 'data',
    field: 'canBufferFrames',
    labelKey: 'settingCanBufferFrames',
    descKey: 'settingCanBufferFramesDesc',
    control: { kind: 'number', min: 1000, max: 500_000, step: 1000 },
  },
  {
    category: 'data',
    field: 'logicBufferSamples',
    labelKey: 'settingLogicBufferSamples',
    descKey: 'settingLogicBufferSamplesDesc',
    control: { kind: 'number', min: 1000, max: 500_000, step: 1000 },
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
