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
  {
    category: 'general',
    field: 'autoCheckUpdate',
    labelKey: 'updateAutoCheck',
    descKey: 'updateAutoCheckDesc',
    control: { kind: 'toggle' },
    keywords: ['update', '更新'],
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
    field: 'reducedMotion',
    labelKey: 'settingReducedMotion',
    descKey: 'settingReducedMotionDesc',
    control: { kind: 'toggle' },
    keywords: ['animation', 'motion', '动画', '动效'],
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
  {
    category: 'editor',
    field: 'pauseOnInteract',
    labelKey: 'settingPauseOnInteract',
    descKey: 'settingPauseOnInteractDesc',
    control: { kind: 'toggle' },
    keywords: ['pause', 'interact', 'zoom', 'pan', '暂停', '缩放', '拖动'],
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
  // Performance
  {
    category: 'performance',
    field: 'maxFeedWorkers',
    labelKey: 'settingMaxFeedWorkers',
    descKey: 'settingMaxFeedWorkersDesc',
    control: { kind: 'number', min: 1, max: 8, step: 1 },
    keywords: ['worker', 'parallel', '并行', '解析'],
  },
  {
    category: 'performance',
    field: 'feedParallelUnit',
    labelKey: 'settingFeedParallelUnit',
    descKey: 'settingFeedParallelUnitDesc',
    control: { kind: 'number', min: 1, max: 256, step: 1 },
    keywords: ['worker', 'parallel', '并行'],
  },
  {
    category: 'performance',
    field: 'minWorkerBytesKb',
    labelKey: 'settingMinWorkerBytesKb',
    descKey: 'settingMinWorkerBytesKbDesc',
    control: { kind: 'number', min: 4, max: 1024, step: 4 },
    keywords: ['worker', 'bytes', '并行'],
  },
  {
    category: 'performance',
    field: 'coalesceMaxMsgs',
    labelKey: 'settingCoalesceMaxMsgs',
    descKey: 'settingCoalesceMaxMsgsDesc',
    control: { kind: 'number', min: 1, max: 1024, step: 1 },
    keywords: ['coalesce', 'batch', '合批'],
  },
  {
    category: 'performance',
    field: 'coalesceMaxBytesKb',
    labelKey: 'settingCoalesceMaxBytesKb',
    descKey: 'settingCoalesceMaxBytesKbDesc',
    control: { kind: 'number', min: 16, max: 4096, step: 16 },
    keywords: ['coalesce', 'batch', '合批', 'buffer'],
  },
  {
    category: 'performance',
    field: 'maxStreamShards',
    labelKey: 'settingMaxStreamShards',
    descKey: 'settingMaxStreamShardsDesc',
    control: { kind: 'number', min: 1, max: 8, step: 1 },
    keywords: ['shard', 'stream', '分片'],
  },
  {
    category: 'performance',
    field: 'parseChannelCap',
    labelKey: 'settingParseChannelCap',
    descKey: 'settingParseChannelCapDesc',
    control: { kind: 'number', min: 16, max: 4096, step: 16 },
    keywords: ['channel', 'buffer', '缓冲', 'parse'],
  },
  // AI (provider 选项与后端 ai_provider::ADAPTERS 注册表保持一致)
  {
    category: 'ai',
    field: 'adapter',
    labelKey: 'settingAiAdapter',
    descKey: 'settingAiAdapterDesc',
    control: {
      kind: 'select',
      options: [
        { value: 'openai_compatible', label: 'OpenAI 兼容 / Compatible' },
        { value: 'openai', label: 'OpenAI' },
        { value: 'anthropic', label: 'Anthropic Claude' },
        { value: 'gemini', label: 'Google Gemini' },
        { value: 'deepseek', label: 'DeepSeek' },
        { value: 'moonshot', label: 'Moonshot Kimi' },
        { value: 'zai', label: '智谱 GLM (Z.ai)' },
        { value: 'bigmodel', label: '智谱 BigModel' },
        { value: 'aliyun', label: '阿里云百炼 (Qwen)' },
        { value: 'openrouter', label: 'OpenRouter' },
        { value: 'groq', label: 'Groq' },
        { value: 'xai', label: 'xAI Grok' },
        { value: 'ollama', label: 'Ollama (本地)' },
      ],
    },
    keywords: ['ai', 'llm', 'provider', '模型', '对话'],
  },
  {
    category: 'ai',
    field: 'baseUrl',
    labelKey: 'settingAiBaseUrl',
    descKey: 'settingAiBaseUrlDesc',
    control: { kind: 'text' },
    keywords: ['ai', 'base url', 'endpoint', '端点', '兼容'],
  },
  {
    category: 'ai',
    field: 'apiKey',
    labelKey: 'settingAiApiKey',
    descKey: 'settingAiApiKeyDesc',
    control: { kind: 'text' },
    keywords: ['ai', 'api key', '密钥', 'token'],
  },
  {
    category: 'ai',
    field: 'model',
    labelKey: 'settingAiModel',
    descKey: 'settingAiModelDesc',
    control: { kind: 'text' },
    keywords: ['ai', 'model', '模型'],
  },
  {
    category: 'ai',
    field: 'temperature',
    labelKey: 'settingAiTemperature',
    descKey: 'settingAiTemperatureDesc',
    control: { kind: 'number', min: 0, max: 2, step: 0.1 },
    keywords: ['ai', 'temperature', '温度', '采样'],
  },
  {
    category: 'ai',
    field: 'maxTokens',
    labelKey: 'settingAiMaxTokens',
    descKey: 'settingAiMaxTokensDesc',
    control: { kind: 'number', min: 16, max: 131072, step: 16 },
    keywords: ['ai', 'tokens', '长度'],
  },
  {
    category: 'ai',
    field: 'systemPrompt',
    labelKey: 'settingAiSystemPrompt',
    descKey: 'settingAiSystemPromptDesc',
    control: { kind: 'text' },
    keywords: ['ai', 'system prompt', '提示词'],
  },
  {
    category: 'ai',
    field: 'maxToolRounds',
    labelKey: 'settingAiMaxToolRounds',
    descKey: 'settingAiMaxToolRoundsDesc',
    control: { kind: 'number', min: 1, max: 50, step: 1 },
    keywords: ['ai', 'tool', '工具', '轮次', 'mcp'],
  },
  {
    category: 'ai',
    field: 'mcpToolsEnabled',
    labelKey: 'settingAiMcpTools',
    descKey: 'settingAiMcpToolsDesc',
    control: { kind: 'toggle' },
    keywords: ['ai', 'mcp', '工具', 'tool'],
  },
  {
    category: 'ai',
    field: 'mcpServerPort',
    labelKey: 'settingAiMcpServerPort',
    descKey: 'settingAiMcpServerPortDesc',
    control: { kind: 'number', min: 1024, max: 65535, step: 1 },
    keywords: ['ai', 'mcp', 'server', '端口', 'port'],
  },
];
