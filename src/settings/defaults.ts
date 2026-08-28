//! 应用设置 schema 与默认值
//!
//! 与 settingsStore.ts 中的 AppSettings 接口对应
//! 通过 tauri-plugin-store 持久化到 app config dir 的 settings.json

import type { Lang } from '../i18n';
import type { CssStyleTheme, ThemeDefinition } from './theme';

/// 应用设置根 schema
export interface AppSettings {
  general: {
    language: Lang;
    autoConnectOnStart: boolean;
    confirmBeforeQuit: boolean;
    showOnboarding: boolean;
    showContextualTips: boolean;
    debug: boolean;
    /** 启动时自动检查更新 */
    autoCheckUpdate: boolean;
    /** 更新通道 (null = 按当前版本推导: 含 '-' 视为预发布 → beta, 否则 stable) */
    updateChannel: 'stable' | 'beta' | null;
    /** 用户选择跳过的更新版本号 (内部字段, 不进设置 UI) */
    skippedUpdateVersion: string | null;
  };
  appearance: {
    theme: string;
    customThemes: ThemeDefinition[];
    /** 当前激活的CSS样式主题ID */
    cssTheme: string;
    /** 用户自定义CSS样式主题列表 */
    customCssThemes: CssStyleTheme[];
    /** 减少动画效果 */
    reducedMotion: boolean;
    uiFontFamily: string;
    uiFontSize: number;
    monoFontFamily: string;
    monoFontSize: number;
    statusBarVisible: boolean;
    activityBarVisible: boolean;
    acrylicBackground: boolean;
    /// 毛玻璃面板透明度 0.1-1.0, 0.6 为基准观感
    acrylicOpacity: number;
  };
  editor: {
    waveformFps: number;
    scopeDefaultTimeBase: number;
    scopeDefaultVPerDiv: number;
    gridVisible: boolean;
    /// 光标读数悬浮框是否显示 (波形图鼠标跟随数值提示)
    cursorReadoutVisible: boolean;
    /// 光标吸附: Y 轴吸附到曲线在鼠标 X 处的插值 (X 轴仍跟随鼠标)
    cursorSnap: boolean;
    /// 十字线可见性 (鼠标跟随的横竖辅助线)
    crosshairVisible: boolean;
    /// 鼠标悬停采样点标记可见性 (曲线上跟随鼠标的数据点圆点)
    hoverPointsVisible: boolean;
    /// 交互时自动暂停: 缩放/拖动波形时自动停止实时刷新 (关闭后交互不打断刷新, 运行中也可拖动回看历史)
    pauseOnInteract: boolean;
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
  /// 数据管道性能调优 — 更新即推送后端 set_pipeline_config (后端不持久化, 启动时重放)
  performance: {
    maxFeedWorkers: number;
    feedParallelUnit: number;
    minWorkerBytesKb: number;
    coalesceMaxMsgs: number;
    coalesceMaxBytesKb: number;
    maxStreamShards: number;
    parseChannelCap: number;
  };
  /// AI 对话与 MCP — api_key 随请求传给后端 (明文存 settings.json, 后端不持久化)
  ai: {
    /** LLM 适配器标识 (见后端 ai_list_providers) */
    adapter: string;
    /** 自定义端点 (空 = provider 默认; openai_compatible 必填) */
    baseUrl: string;
    /** API key */
    apiKey: string;
    /** 模型名 */
    model: string;
    /** 采样温度 (null = provider 默认) */
    temperature: number | null;
    /** 最大生成 token (null = provider 默认) */
    maxTokens: number | null;
    /** 系统提示词 (空 = 不发送) */
    systemPrompt: string;
    /** 工具调用循环最大轮次 */
    maxToolRounds: number;
    /** 对话中是否启用 MCP 工具 */
    mcpToolsEnabled: boolean;
    /** 本地 MCP server 端口 (127.0.0.1, 供外部 AI 客户端连接) */
    mcpServerPort: number;
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
    autoCheckUpdate: true,
    updateChannel: null,
    skippedUpdateVersion: null,
  },
  appearance: {
    theme: 'dark',
    customThemes: [],
    cssTheme: 'default',
    customCssThemes: [],
    reducedMotion: false,
    uiFontFamily: "'JetBrains Mono', 'Maple Mono CN', -apple-system, BlinkMacSystemFont, 'Segoe UI', 'PingFang SC', 'Microsoft YaHei', sans-serif",
    uiFontSize: 13,
    monoFontFamily: "'JetBrains Mono', 'Maple Mono CN', 'Cascadia Code', 'Fira Code', 'SF Mono', Menlo, 'PingFang SC', 'Microsoft YaHei', monospace",
    monoFontSize: 12,
    statusBarVisible: true,
    activityBarVisible: true,
    acrylicBackground: false,
    acrylicOpacity: 0.6,
  },
  editor: {
    waveformFps: 30,
    scopeDefaultTimeBase: 100e-3,
    scopeDefaultVPerDiv: 1,
    gridVisible: true,
    cursorReadoutVisible: true,
    cursorSnap: true,
    crosshairVisible: true,
    hoverPointsVisible: true,
    pauseOnInteract: false,
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
  performance: {
    maxFeedWorkers: 4,
    feedParallelUnit: 8,
    minWorkerBytesKb: 32,
    coalesceMaxMsgs: 64,
    coalesceMaxBytesKb: 256,
    maxStreamShards: 4,
    parseChannelCap: 256,
  },
  ai: {
    adapter: 'openai_compatible',
    baseUrl: '',
    apiKey: '',
    model: '',
    temperature: null,
    maxTokens: null,
    systemPrompt: '',
    maxToolRounds: 10,
    mcpToolsEnabled: true,
    mcpServerPort: 8765,
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
  { key: 'performance', icon: 'Gauge' },
  { key: 'ai', icon: 'Sparkles' },
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
