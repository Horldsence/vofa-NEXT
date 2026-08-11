//! 设置 store — 基于 zustand + tauri-plugin-store
//!
//! 启动时调用 load() 从磁盘加载, 每次 update 后 save() (防抖 300ms)
//! 通过 subscribeAppearance() 自动应用 appearance 到 CSS 变量

import { create } from 'zustand';
import { LazyStore } from '@tauri-apps/plugin-store';
import {
  AppSettings,
  DEFAULT_SETTINGS,
  deepMergeSettings,
} from '../settings/defaults';
import { applyAppearance } from '../settings/applyTheme';
import {
  DARK_THEME,
  THEME_TOKENS,
  type ThemeDefinition,
  type ThemeToken,
} from '../settings/theme';
import { api } from '../lib/tauri/tauri';
import { rawDataBuffer } from '../lib/buffers/dataBuffer';
import { canFrameBuffer } from '../lib/buffers/canBuffer';
import { logicSampleBuffer } from '../lib/buffers/logicBuffer';
import { transitionStore } from '../lib/utils/transitionStore';

const STORE_FILE = 'settings.json';
const STORE_KEY = 'app';

/// 单例 LazyStore — 多次调用共享底层连接
let storeInstance: LazyStore | null = null;
function getStore(): LazyStore {
  if (!storeInstance) storeInstance = new LazyStore(STORE_FILE);
  return storeInstance;
}

/// 防抖保存计时器
let saveTimer: ReturnType<typeof setTimeout> | null = null;

interface SettingsStore {
  settings: AppSettings;
  isOpen: boolean;
  isAboutOpen: boolean;
  activeCategory: keyof AppSettings;
  searchQuery: string;
  loaded: boolean;

  open: (category?: keyof AppSettings) => void;
  close: () => void;
  openAbout: () => void;
  closeAbout: () => void;
  setActiveCategory: (c: keyof AppSettings) => void;
  setSearchQuery: (q: string) => void;

  load: () => Promise<void>;
  update: <K extends keyof AppSettings>(
    category: K,
    field: keyof AppSettings[K],
    value: AppSettings[K][keyof AppSettings[K]]
  ) => void;
  reset: () => void;
  resetCategory: (category: keyof AppSettings) => void;
}

/// 将 data 分类的缓存容量设置同步到后端与前端 buffer 实例
function applyDataCapacity(settings: AppSettings) {
  const data = settings.data;
  api.setWaveformBufferCapacity(data.waveformBufferPoints).catch((e: unknown) =>
    console.warn('[settings] 设置波形缓冲区容量失败:', e)
  );
  api.setRawDataBufferCapacity(data.rawDataBufferBytes).catch((e: unknown) =>
    console.warn('[settings] 设置原始数据缓冲区容量失败:', e)
  );
  api.setCanBufferCapacity(data.canBufferFrames).catch((e: unknown) =>
    console.warn('[settings] 设置 CAN 缓冲区容量失败:', e)
  );
  api.setLogicBufferCapacity(data.logicBufferSamples).catch((e: unknown) =>
    console.warn('[settings] 设置逻辑缓冲区容量失败:', e)
  );

  // 前端 buffer 实例同步容量 (后端容量调整不自动影响前端缓存)
  rawDataBuffer.setCapacity(data.rawDataBufferBytes);
  canFrameBuffer.setCapacity(data.canBufferFrames);
  logicSampleBuffer.setCapacity(data.logicBufferSamples);
}

/// 历史版本默认字体 — 若用户未自定义过(仍是旧默认), 迁移到最新默认
const LEGACY_DEFAULT_UI_FONTS = [
  "-apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif",
  "'JetBrains Mono', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif",
];
const LEGACY_DEFAULT_MONO_FONTS = [
  "'Cascadia Code', 'Fira Code', 'SF Mono', Menlo, monospace",
  "'JetBrains Mono', 'Cascadia Code', 'Fira Code', 'SF Mono', Menlo, monospace",
];

/// 防御性 token 迁移映射: 历史版本曾用过的旧 token 名 -> 当前名。
/// 当前语义层为纯增量 (未重命名任何可持久化 token), 映射保持为空;
/// 未来若重命名 THEME_TOKENS 中的 key, 在此补条目即可平滑迁移已保存的自定义主题。
const LEGACY_TOKEN_RENAMES: Readonly<Record<string, ThemeToken>> = {};

/// 归一化自定义主题: 应用旧 key 重命名 + 补齐缺失 token。
/// 防御旧版本/损坏数据 — 保证每个主题都持有完整 token 集合 (新语义结构)。
export function migrateCustomTheme(theme: ThemeDefinition): ThemeDefinition {
  const tokens: Record<string, string> = { ...theme.tokens };
  for (const [oldKey, newKey] of Object.entries(LEGACY_TOKEN_RENAMES)) {
    if (oldKey in tokens && !(newKey in tokens)) {
      tokens[newKey] = tokens[oldKey];
    }
    delete tokens[oldKey];
  }
  for (const token of THEME_TOKENS) {
    if (!(token in tokens)) {
      tokens[token] = DARK_THEME.tokens[token];
    }
  }
  return { ...theme, tokens: tokens as Record<ThemeToken, string> };
}

function migrateSettings(settings: AppSettings): AppSettings {
  const appearance = { ...settings.appearance };
  if (LEGACY_DEFAULT_UI_FONTS.includes(appearance.uiFontFamily)) {
    appearance.uiFontFamily = DEFAULT_SETTINGS.appearance.uiFontFamily;
  }
  if (LEGACY_DEFAULT_MONO_FONTS.includes(appearance.monoFontFamily)) {
    appearance.monoFontFamily = DEFAULT_SETTINGS.appearance.monoFontFamily;
  }
  if (appearance.customThemes?.length) {
    appearance.customThemes = appearance.customThemes.map(migrateCustomTheme);
  }
  return { ...settings, appearance };
}

export const useSettingsStore = create<SettingsStore>((set, get) => ({
  settings: DEFAULT_SETTINGS,
  isOpen: false,
  isAboutOpen: false,
  activeCategory: 'general',
  searchQuery: '',
  loaded: false,

  open: (category) =>
    set({
      isOpen: true,
      activeCategory: category ?? get().activeCategory,
      searchQuery: '',
    }),
  close: () => set({ isOpen: false }),
  openAbout: () => set({ isAboutOpen: true }),
  closeAbout: () => set({ isAboutOpen: false }),
  setActiveCategory: (c) => set({ activeCategory: c }),
  setSearchQuery: (q) => set({ searchQuery: q }),

  load: async () => {
    try {
      const raw = await getStore().get<AppSettings>(STORE_KEY);
      if (raw) {
        // 与默认值合并, 防止新版本缺失字段
        const merged = migrateSettings(deepMergeSettings(DEFAULT_SETTINGS, raw));
        set({ settings: merged, loaded: true });
        applyAppearance(merged.appearance);
        applyDataCapacity(merged);
      } else {
        set({ loaded: true });
        applyAppearance(DEFAULT_SETTINGS.appearance);
        applyDataCapacity(DEFAULT_SETTINGS);
      }
    } catch (e) {
      console.warn('[settings] 加载失败, 使用默认值:', e);
      set({ loaded: true });
      applyAppearance(DEFAULT_SETTINGS.appearance);
      applyDataCapacity(DEFAULT_SETTINGS);
    }
  },

  update: (category, field, value) => {
    // 外观/主题应用 (CSS 变量 + 全量重绘) 为非紧急更新 — 延迟到 transition 渲染。
    // 调用点 SettingsModal 不在本次变更范围, 故在 store 动作内包装。
    const commit = () => {
      set((s) => {
        const newSettings: AppSettings = {
          ...s.settings,
          [category]: {
            ...s.settings[category],
            [field]: value,
          },
        };
        // 异步保存 (防抖 300ms)
        if (saveTimer) clearTimeout(saveTimer);
        saveTimer = setTimeout(() => {
          getStore()
            .set(STORE_KEY, get().settings)
            .catch((e: unknown) => console.warn('[settings] 保存失败:', e));
        }, 300);
        // 立即应用 appearance 变更
        if (category === 'appearance') {
          applyAppearance(newSettings.appearance);
        }
        // 立即应用 data 缓存容量变更
        if (category === 'data') {
          applyDataCapacity(newSettings);
        }
        return { settings: newSettings };
      });
    };
    if (category === 'appearance') {
      transitionStore(commit);
    } else {
      commit();
    }
  },

  reset: () => {
    set({ settings: DEFAULT_SETTINGS });
    applyAppearance(DEFAULT_SETTINGS.appearance);
    applyDataCapacity(DEFAULT_SETTINGS);
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => {
      getStore()
        .set(STORE_KEY, DEFAULT_SETTINGS)
        .catch((e: unknown) => console.warn('[settings] 保存失败:', e));
    }, 300);
  },

  resetCategory: (category) => {
    set((s) => ({
      settings: {
        ...s.settings,
        [category]: JSON.parse(JSON.stringify(DEFAULT_SETTINGS[category])),
      },
    }));
    const { settings } = get();
    if (category === 'appearance') applyAppearance(settings.appearance);
    if (category === 'data') applyDataCapacity(settings);
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => {
      getStore()
        .set(STORE_KEY, get().settings)
        .catch((e: unknown) => console.warn('[settings] 保存失败:', e));
    }, 300);
  },
}));
