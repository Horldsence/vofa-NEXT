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
import { api } from '../lib/tauri';
import { rawDataBuffer } from '../lib/dataBuffer';
import { canFrameBuffer } from '../lib/canBuffer';
import { logicSampleBuffer } from '../lib/logicBuffer';

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
        const merged = deepMergeSettings(DEFAULT_SETTINGS, raw);
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
