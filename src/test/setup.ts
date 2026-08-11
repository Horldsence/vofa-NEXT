//! Vitest 全局 setup — 注册 jest-dom 匹配器 + @tauri-apps/* mock 桩
//!
//! 目标: 让 components / stores / lib 模块可以在没有 Tauri 运行时的 jsdom 中
//! 被导入与测试, 而不触发真实 Tauri API (这些 API 在非 Tauri 环境会抛错).
//!
//! 用法: 测试中通过 `import { tauriMock } from '../../test/setup'` 访问共享状态,
//! 例如 seedFile() 预置 plugin-store 数据、断言 invoke 调用.

import '@testing-library/jest-dom/vitest';
import { vi } from 'vitest';

/// 共享、可重置的 mock 状态 — 通过 vi.hoisted 保证在 vi.mock 工厂中可用
const tauriMock = vi.hoisted(() => {
  /** plugin-store 的底层存储: file -> (key -> value) */
  const fileStore = new Map<string, Map<string, unknown>>();

  return {
    fileStore,

    /** 预置某个 store 文件/键的数据 (供 load 路径测试) */
    seedFile: (file: string, key: string, value: unknown) => {
      let entry = fileStore.get(file);
      if (!entry) {
        entry = new Map();
        fileStore.set(file, entry);
      }
      entry.set(key, value);
    },

    // ---- @tauri-apps/api/core ----
    invoke: vi.fn(async () => undefined),

    // ---- @tauri-apps/api/event ----
    listen: vi.fn(async () => () => {}),
    emit: vi.fn(async () => undefined),

    // ---- @tauri-apps/plugin-log ----
    logTrace: vi.fn(),
    logDebug: vi.fn(),
    logInfo: vi.fn(),
    logWarn: vi.fn(),
    logError: vi.fn(),

    // ---- @tauri-apps/plugin-dialog ----
    dialogSave: vi.fn(async () => null),
    dialogOpen: vi.fn(async () => null),

    // ---- @tauri-apps/plugin-notification ----
    isPermissionGranted: vi.fn(async () => true),
    requestPermission: vi.fn(async () => 'granted'),
    sendNotification: vi.fn(),

    // ---- @tauri-apps/plugin-fs ----
    readTextFile: vi.fn(async () => ''),
    writeTextFile: vi.fn(async () => undefined),

    // ---- @tauri-apps/plugin-opener ----
    openUrl: vi.fn(async () => undefined),

    // ---- @tauri-apps/plugin-process ----
    relaunch: vi.fn(async () => undefined),
    exit: vi.fn(async () => undefined),

    // ---- @tauri-apps/plugin-updater ----
    check: vi.fn(async () => null),
  };
});

export { tauriMock };

/// jsdom 未实现 window.matchMedia — uPlot 模块初始化 / AnimatedSwitch 动画均依赖。
/// 在 setup 阶段 (早于任何被测模块 import) 注入, 避免组件树加载时抛错。
vi.stubGlobal(
  'matchMedia',
  (query: string): MediaQueryList =>
    ({
      matches: false,
      media: query,
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    }) as unknown as MediaQueryList
);

/// 最小 Channel 桩 — 仅需可构造、含 id/onmessage 即可
function createChannelStub() {
  return class Channel<T = unknown> {
    id: number;
    onmessage: ((message: T) => void) | null = null;

    constructor(id?: number) {
      this.id = id ?? 0;
    }
  };
}

vi.mock('@tauri-apps/api', () => ({
  invoke: tauriMock.invoke,
  listen: tauriMock.listen,
  emit: tauriMock.emit,
  Channel: createChannelStub(),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: tauriMock.invoke,
  Channel: createChannelStub(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: tauriMock.listen,
  emit: tauriMock.emit,
}));

vi.mock('@tauri-apps/plugin-log', () => ({
  trace: tauriMock.logTrace,
  debug: tauriMock.logDebug,
  info: tauriMock.logInfo,
  warn: tauriMock.logWarn,
  error: tauriMock.logError,
}));

vi.mock('@tauri-apps/plugin-store', () => {
  class FakeLazyStore {
    private readonly file: string;

    constructor(file: string) {
      this.file = file;
    }

    async get<T>(key: string): Promise<T | null> {
      const entry = tauriMock.fileStore.get(this.file);
      return (entry?.get(key) as T | undefined) ?? null;
    }

    async set(key: string, value: unknown): Promise<void> {
      let entry = tauriMock.fileStore.get(this.file);
      if (!entry) {
        entry = new Map();
        tauriMock.fileStore.set(this.file, entry);
      }
      entry.set(key, value);
    }

    async save(): Promise<void> {}

    async load(): Promise<void> {}
  }

  return { LazyStore: FakeLazyStore, Store: FakeLazyStore };
});

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: tauriMock.dialogOpen,
  save: tauriMock.dialogSave,
  ask: vi.fn(async () => true),
  confirm: vi.fn(async () => true),
  message: vi.fn(async () => undefined),
}));

vi.mock('@tauri-apps/plugin-notification', () => ({
  isPermissionGranted: tauriMock.isPermissionGranted,
  requestPermission: tauriMock.requestPermission,
  sendNotification: tauriMock.sendNotification,
}));

vi.mock('@tauri-apps/plugin-fs', () => ({
  readTextFile: tauriMock.readTextFile,
  writeTextFile: tauriMock.writeTextFile,
  readFile: vi.fn(async () => new Uint8Array()),
  writeFile: vi.fn(async () => undefined),
  exists: vi.fn(async () => true),
  mkdir: vi.fn(async () => undefined),
  remove: vi.fn(async () => undefined),
}));

vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: tauriMock.openUrl,
  openPath: vi.fn(async () => undefined),
  revealItemInDir: vi.fn(async () => undefined),
}));

vi.mock('@tauri-apps/plugin-process', () => ({
  relaunch: tauriMock.relaunch,
  exit: tauriMock.exit,
}));

vi.mock('@tauri-apps/plugin-updater', () => ({
  check: tauriMock.check,
  download: vi.fn(async () => undefined),
  install: vi.fn(async () => undefined),
}));
