//! updateStore 测试 — 自动/手动检查更新的状态机与通道解析
//!
//! tauri core/process 由 test/setup.ts 全局 mock;
//! 此处覆盖 @tauri-apps/api/event 为可捕获回调的实现 (便于模拟后端 emit),
//! 并补充 @tauri-apps/api/app (getVersion 决定默认通道).
//! settingsStore 使用真实实现 (已有全局 tauri mock 支撑), 以便验证 skipVersion 落盘路径.

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { tauriMock } from './setup';
import { DEFAULT_SETTINGS } from '../settings/defaults';
import { useSettingsStore } from '../store/settingsStore';
import { useUpdateStore } from '../store/updateStore';

/// 覆盖 setup.ts 的 event mock — 捕获 listen 回调, 测试可模拟后端 emit
/// (hoisted 保证在模块加载期注册的 update://ready 监听也被捕获)
const eventMock = vi.hoisted(() => {
  type ListenHandler = (event: { payload: unknown }) => void;
  const handlers = new Map<string, ListenHandler>();
  return {
    handlers,
    listen: vi.fn((name: string, cb: ListenHandler) => {
      handlers.set(name, cb);
      return Promise.resolve(() => undefined);
    }),
    emit: vi.fn(() => Promise.resolve(undefined)),
  };
});

vi.mock('@tauri-apps/api/event', () => ({
  listen: eventMock.listen,
  emit: eventMock.emit,
}));

/// @tauri-apps/api/app 不在全局 setup 中 — 按测试需要 mock getVersion
const appApiMock = vi.hoisted(() => ({
  getVersion: vi.fn(() => Promise.resolve('0.1.9')),
}));

vi.mock('@tauri-apps/api/app', () => ({
  getVersion: appApiMock.getVersion,
  getName: vi.fn(() => Promise.resolve('VOFA-Next')),
}));

/// 模拟后端 emit 事件
function emitTauri(name: string, payload: unknown) {
  eventMock.handlers.get(name)?.({ payload });
}

/// check_update 的标准"有更新"响应
const AVAILABLE_RESPONSE = {
  available: true,
  currentVersion: '0.1.9',
  version: '0.2.0',
  notes: 'new stuff',
  date: '2026-08-01',
};

const UP_TO_DATE_RESPONSE = {
  available: false,
  currentVersion: '0.1.9',
  version: null,
  notes: null,
  date: null,
};

/// tauriMock.invoke 在 setup 中声明为零参签名 — 统一经此助手注入带命令参数的实现
function mockInvoke(impl: (cmd: string, args?: unknown) => unknown) {
  tauriMock.invoke.mockImplementation(impl as unknown as () => Promise<undefined>);
}

/// invoke 调用中 check_update 的通道参数
function checkUpdateChannels(): unknown[] {
  return (tauriMock.invoke.mock.calls as unknown as [string, ...unknown[]][])
    .filter(([cmd]) => cmd === 'check_update')
    .map(([, args]) => (args as { channel: unknown }).channel);
}

function resetStores() {
  useSettingsStore.setState({
    settings: structuredClone(DEFAULT_SETTINGS),
    loaded: true,
  });
  useUpdateStore.setState({
    status: 'idle',
    lastTrigger: 'manual',
    updateInfo: null,
    currentVersion: '',
    progress: 0,
    error: null,
    dialogOpen: false,
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  appApiMock.getVersion.mockResolvedValue('0.1.9');
  resetStores();
});

describe('updateStore.check', () => {
  it('auto 检查发现更新且未被跳过 → available + 自动打开弹窗', async () => {
    mockInvoke((cmd: string) =>
      cmd === 'check_update' ? AVAILABLE_RESPONSE : undefined
    );

    await useUpdateStore.getState().check('auto');

    const s = useUpdateStore.getState();
    expect(s.status).toBe('available');
    expect(s.lastTrigger).toBe('auto');
    expect(s.updateInfo).toEqual({ version: '0.2.0', notes: 'new stuff', date: '2026-08-01' });
    expect(s.currentVersion).toBe('0.1.9');
    expect(s.dialogOpen).toBe(true);
    expect(s.error).toBeNull();
  });

  it('auto 检查发现更新但版本已被跳过 → available 但不打开弹窗', async () => {
    useSettingsStore.getState().update('general', 'skippedUpdateVersion', '0.2.0');
    mockInvoke((cmd: string) =>
      cmd === 'check_update' ? AVAILABLE_RESPONSE : undefined
    );

    await useUpdateStore.getState().check('auto');

    const s = useUpdateStore.getState();
    expect(s.status).toBe('available');
    expect(s.dialogOpen).toBe(false);
  });

  it('manual 检查发现更新 → 不自动打开弹窗', async () => {
    mockInvoke((cmd: string) =>
      cmd === 'check_update' ? AVAILABLE_RESPONSE : undefined
    );

    await useUpdateStore.getState().check('manual');

    const s = useUpdateStore.getState();
    expect(s.status).toBe('available');
    expect(s.dialogOpen).toBe(false);
  });

  it('检查无更新 → up-to-date', async () => {
    mockInvoke((cmd: string) =>
      cmd === 'check_update' ? UP_TO_DATE_RESPONSE : undefined
    );

    await useUpdateStore.getState().check('auto');

    const s = useUpdateStore.getState();
    expect(s.status).toBe('up-to-date');
    expect(s.updateInfo).toBeNull();
    expect(s.dialogOpen).toBe(false);
  });

  it('检查失败 (reject) → error 状态并记录错误消息', async () => {
    mockInvoke((cmd: string) => {
      if (cmd === 'check_update') throw new Error('network unreachable');
      return undefined;
    });

    await useUpdateStore.getState().check('auto');

    const s = useUpdateStore.getState();
    expect(s.status).toBe('error');
    expect(s.error).toBe('network unreachable');
    expect(s.dialogOpen).toBe(false);
  });

  it('updateChannel=null 且当前版本含 "-" → 以 beta 通道检查', async () => {
    appApiMock.getVersion.mockResolvedValue('0.1.9-beta.2');
    mockInvoke(() => UP_TO_DATE_RESPONSE);

    await useUpdateStore.getState().check('auto');

    expect(checkUpdateChannels()).toEqual(['beta']);
  });

  it('updateChannel=null 且当前版本不含 "-" → 以 stable 通道检查', async () => {
    appApiMock.getVersion.mockResolvedValue('0.1.9');
    mockInvoke(() => UP_TO_DATE_RESPONSE);

    await useUpdateStore.getState().check('auto');

    expect(checkUpdateChannels()).toEqual(['stable']);
  });

  it('显式设置的 updateChannel 优先于版本推导', async () => {
    useSettingsStore.getState().update('general', 'updateChannel', 'beta');
    appApiMock.getVersion.mockResolvedValue('0.1.9');
    mockInvoke(() => UP_TO_DATE_RESPONSE);

    await useUpdateStore.getState().check('auto');

    expect(checkUpdateChannels()).toEqual(['beta']);
  });
});

describe('updateStore.skipVersion', () => {
  it('写入 skippedUpdateVersion 设置、关闭弹窗并回到 idle', async () => {
    mockInvoke((cmd: string) =>
      cmd === 'check_update' ? AVAILABLE_RESPONSE : undefined
    );
    await useUpdateStore.getState().check('auto');
    expect(useUpdateStore.getState().dialogOpen).toBe(true);

    useUpdateStore.getState().skipVersion();

    const s = useUpdateStore.getState();
    expect(s.status).toBe('idle');
    expect(s.dialogOpen).toBe(false);
    expect(useSettingsStore.getState().settings.general.skippedUpdateVersion).toBe('0.2.0');
  });
});

describe('updateStore.downloadAndInstall', () => {
  it('按 update://progress 事件更新进度, invoke 完成后进入 ready', async () => {
    mockInvoke((cmd: string) => {
      if (cmd === 'download_and_install_update') {
        emitTauri('update://progress', { received: 50, total: 100 });
        // total 为 null 时不确定态 — 进度保持不变
        emitTauri('update://progress', { received: 60, total: null });
      }
      return undefined;
    });

    // 订阅下载期间的进度变化, 验证事件确实推进了进度
    const seen: number[] = [];
    const unsub = useUpdateStore.subscribe((s) => {
      if (s.status === 'downloading') seen.push(s.progress);
    });
    await useUpdateStore.getState().downloadAndInstall();
    unsub();

    expect(seen).toEqual([0, 50]);
    const s = useUpdateStore.getState();
    expect(s.status).toBe('ready');
    expect(s.progress).toBe(100);
    expect(s.error).toBeNull();
  });

  it('update://ready 事件先于 invoke resolve 到达时也进入 ready', async () => {
    mockInvoke((cmd: string) => {
      if (cmd === 'download_and_install_update') {
        emitTauri('update://ready', null);
      }
      return undefined;
    });

    await useUpdateStore.getState().downloadAndInstall();

    expect(useUpdateStore.getState().status).toBe('ready');
  });

  it('下载安装失败 (reject) → error 状态', async () => {
    mockInvoke((cmd: string) => {
      if (cmd === 'download_and_install_update') throw new Error('download failed');
      return undefined;
    });

    await useUpdateStore.getState().downloadAndInstall();

    const s = useUpdateStore.getState();
    expect(s.status).toBe('error');
    expect(s.error).toBe('download failed');
  });
});

describe('updateStore.setChannel', () => {
  it('写入 updateChannel 设置并触发 manual 检查', async () => {
    mockInvoke((cmd: string) =>
      cmd === 'check_update' ? AVAILABLE_RESPONSE : undefined
    );

    useUpdateStore.getState().setChannel('beta');
    // check 是异步的 — 等待 invoke 完成
    await vi.waitFor(() => {
      expect(useUpdateStore.getState().status).toBe('available');
    });

    expect(useSettingsStore.getState().settings.general.updateChannel).toBe('beta');
    expect(useUpdateStore.getState().lastTrigger).toBe('manual');
    expect(checkUpdateChannels()).toEqual(['beta']);
  });
});
