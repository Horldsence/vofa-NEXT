import { beforeEach, describe, expect, it, vi } from 'vitest';
import { tauriMock } from '../../test/setup';
import { DEFAULT_SETTINGS, type AppSettings } from '../../settings/defaults';
import { useSettingsStore } from '../settingsStore';

const STORE_FILE = 'settings.json';
const STORE_KEY = 'app';

/// 提取 set_pipeline_config 的调用参数 (invoke mock 无参数签名, 需显式收窄)
function pipelineConfigCalls(): { config: Record<string, number | string> }[] {
  return (tauriMock.invoke.mock.calls as unknown as [string, ...unknown[]][])
    .filter(([cmd]) => cmd === 'set_pipeline_config')
    .map(([, args]) => args as { config: Record<string, number> });
}

describe('settingsStore', () => {
  beforeEach(() => {
    tauriMock.fileStore.clear();
    vi.clearAllMocks();
    useSettingsStore.setState({
      settings: DEFAULT_SETTINGS,
      isOpen: false,
      isAboutOpen: false,
      activeCategory: 'general',
      searchQuery: '',
      loaded: false,
      keychainPermissionPromptOpen: false,
      keychainPermissionRetrying: false,
      keychainPermissionRetryError: null,
    });
  });

  it('loads persisted settings from the LazyStore and merges them over defaults', async () => {
    tauriMock.seedFile(STORE_FILE, STORE_KEY, { general: { language: 'en' } });

    await useSettingsStore.getState().load();

    const { settings, loaded } = useSettingsStore.getState();
    expect(loaded).toBe(true);
    expect(settings.general.language).toBe('en');
    expect(settings.general.showOnboarding).toBe(DEFAULT_SETTINGS.general.showOnboarding);
  });

  it('falls back to defaults when the store holds no saved settings', async () => {
    await useSettingsStore.getState().load();

    const { settings, loaded } = useSettingsStore.getState();
    expect(loaded).toBe(true);
    expect(settings.general.language).toBe(DEFAULT_SETTINGS.general.language);
    expect(settings.appearance.uiFontSize).toBe(DEFAULT_SETTINGS.appearance.uiFontSize);
  });

  it('toggles modal state and category via open/close slice actions', () => {
    useSettingsStore.getState().open('serial');
    expect(useSettingsStore.getState().isOpen).toBe(true);
    expect(useSettingsStore.getState().activeCategory).toBe('serial');

    useSettingsStore.getState().setActiveCategory('appearance');
    expect(useSettingsStore.getState().activeCategory).toBe('appearance');

    useSettingsStore.getState().close();
    expect(useSettingsStore.getState().isOpen).toBe(false);
  });

  it('pushes pipeline config on load (backend does not persist it)', async () => {
    await useSettingsStore.getState().load();

    const calls = pipelineConfigCalls();
    expect(calls).toHaveLength(1);
    expect(calls[0]).toEqual({
      config: {
        mode: 'auto',
        max_workers: DEFAULT_SETTINGS.performance.maxWorkers,
        eval_workers: DEFAULT_SETTINGS.performance.evalWorkers,
        memory_budget_mb: DEFAULT_SETTINGS.performance.memoryBudgetMb,
        preview_fps_limit: DEFAULT_SETTINGS.performance.previewFpsLimit,
        preview_bandwidth_mb_per_sec: DEFAULT_SETTINGS.performance.previewBandwidthMbPerSec,
      },
    });
  });

  it('pushes pipeline config immediately when a performance setting is updated', () => {
    useSettingsStore.getState().update('performance', 'maxWorkers', 12);

    const calls = pipelineConfigCalls();
    expect(calls).toHaveLength(1);
    expect(calls[0].config.max_workers).toBe(12);
    expect(useSettingsStore.getState().settings.performance.maxWorkers).toBe(12);
  });

  it('migrates legacy manual pipeline knobs to automatic safety limits', async () => {
    tauriMock.seedFile(STORE_FILE, STORE_KEY, {
      performance: {
        maxFeedWorkers: 2,
        feedParallelUnit: 4,
        minWorkerBytesKb: 16,
        coalesceMaxMsgs: 8,
        coalesceMaxBytesKb: 32,
        maxStreamShards: 4,
        parseChannelCap: 64,
      },
    });

    await useSettingsStore.getState().load();

    expect(useSettingsStore.getState().settings.performance).toEqual(DEFAULT_SETTINGS.performance);
    const saved = tauriMock.fileStore.get(STORE_FILE)?.get(STORE_KEY) as {
      performance: Record<string, unknown>;
    };
    expect(saved.performance).toEqual(DEFAULT_SETTINGS.performance);
    expect(saved.performance).not.toHaveProperty('maxStreamShards');
  });

  it('does not push pipeline config when a non-performance setting is updated', () => {
    useSettingsStore.getState().update('notifications', 'duration', 3000);

    expect(pipelineConfigCalls()).toHaveLength(0);
  });
});

describe('settingsStore API key 钥匙串集成', () => {
  beforeEach(() => {
    tauriMock.fileStore.clear();
    vi.clearAllMocks();
    useSettingsStore.setState({
      settings: DEFAULT_SETTINGS,
      loaded: false,
      keychainPermissionPromptOpen: false,
      keychainPermissionRetrying: false,
      keychainPermissionRetryError: null,
    });
  });

  function keychainCalls(cmd: string): [string, string][] {
    return (tauriMock.invoke.mock.calls as unknown as [string, ...unknown[]][])
      .filter(([c]) => c === cmd)
      .map(([, args]) => args as { adapter: string; key?: string })
      .map((a) => [a.adapter, a.key ?? '']);
  }

  it('load 时从钥匙串水合当前适配器的 API key', async () => {
    tauriMock.seedFile(STORE_FILE, STORE_KEY, {});
    (tauriMock.invoke as unknown as { mockImplementation: (f: (cmd: string) => unknown) => void })
      .mockImplementation((cmd: string) => Promise.resolve(cmd === 'ai_keychain_get' ? 'sk-stored' : undefined));

    await useSettingsStore.getState().load();

    expect(useSettingsStore.getState().settings.ai.apiKey).toBe('sk-stored');
  });

  it('旧版明文 key 自动迁入钥匙串 (仅当钥匙串为空)', async () => {
    tauriMock.seedFile(STORE_FILE, STORE_KEY, { ai: { apiKey: 'sk-legacy' } });
    (tauriMock.invoke as unknown as { mockImplementation: (f: (cmd: string) => unknown) => void })
      .mockImplementation((cmd: string) => Promise.resolve(cmd === 'ai_keychain_get' ? null : undefined));

    await useSettingsStore.getState().load();

    expect(keychainCalls('ai_keychain_set')).toContainEqual(['orcarouter', 'sk-legacy']);
    expect(useSettingsStore.getState().settings.ai.apiKey).toBe('sk-legacy');
  });

  it('持久化剥离 API key — settings.json 恒为空串', async () => {
    useSettingsStore.setState({
      settings: {
        ...DEFAULT_SETTINGS,
        ai: { ...DEFAULT_SETTINGS.ai, adapter: 'orcarouter', apiKey: 'sk-secret', model: 'openai/gpt-4o-mini' },
      },
    });
    useSettingsStore.getState().update('general', 'language', 'en');
    await vi.waitFor(() => {
      const saved = tauriMock.fileStore.get(STORE_FILE)?.get(STORE_KEY) as { ai?: { apiKey?: string } } | undefined;
      expect(saved?.ai?.apiKey).toBe('');
    });
    // 内存中保留真实值 (随请求传给后端)
    expect(useSettingsStore.getState().settings.ai.apiKey).toBe('sk-secret');
  });

  it('update apiKey 写入钥匙串; 切换适配器水合对应密钥', () => {
    useSettingsStore.getState().update('ai', 'apiKey', 'sk-new');
    expect(keychainCalls('ai_keychain_set')).toContainEqual(['orcarouter', 'sk-new']);

    (tauriMock.invoke as unknown as { mockImplementation: (f: (cmd: string) => unknown) => void })
      .mockImplementation((cmd: string) => Promise.resolve(cmd === 'ai_keychain_get' ? 'sk-deepseek' : undefined));
    useSettingsStore.getState().update('ai', 'adapter', 'deepseek');
    return vi.waitFor(() => {
      expect(useSettingsStore.getState().settings.ai.apiKey).toBe('sk-deepseek');
    });
  });

  it('启动读取被拒绝时打开授权提醒', async () => {
    (tauriMock.invoke as unknown as {
      mockImplementation: (f: (cmd: string) => unknown) => void;
    }).mockImplementation((cmd: string) => {
      if (cmd === 'ai_keychain_get') {
        return Promise.reject(Object.assign(new Error('cancelled'), { kind: 'AiKeyringAccessDenied', data: {} }));
      }
      return Promise.resolve(undefined);
    });

    await useSettingsStore.getState().load();

    expect(useSettingsStore.getState().keychainPermissionPromptOpen).toBe(true);
    expect(useSettingsStore.getState().settings.ai.apiKey).toBe('');
  });

  it('普通钥匙串故障不打开授权提醒', async () => {
    (tauriMock.invoke as unknown as {
      mockImplementation: (f: (cmd: string) => unknown) => void;
    }).mockImplementation((cmd: string) => {
      if (cmd === 'ai_keychain_get') {
        return Promise.reject(Object.assign(new Error('locked'), { kind: 'AiKeyring', data: {} }));
      }
      return Promise.resolve(undefined);
    });

    await useSettingsStore.getState().load();

    expect(useSettingsStore.getState().keychainPermissionPromptOpen).toBe(false);
  });

  it('已选择不再提醒时忽略启动授权拒绝', async () => {
    tauriMock.seedFile(STORE_FILE, STORE_KEY, {
      general: { suppressKeychainPermissionReminder: true },
    });
    (tauriMock.invoke as unknown as {
      mockImplementation: (f: (cmd: string) => unknown) => void;
    }).mockImplementation((cmd: string) => {
      if (cmd === 'ai_keychain_get') {
        return Promise.reject(Object.assign(new Error('cancelled'), { kind: 'AiKeyringAccessDenied', data: {} }));
      }
      return Promise.resolve(undefined);
    });

    await useSettingsStore.getState().load();

    expect(useSettingsStore.getState().keychainPermissionPromptOpen).toBe(false);
  });

  it('再次请求成功后水合密钥并关闭提醒', async () => {
    useSettingsStore.setState({ keychainPermissionPromptOpen: true });
    (tauriMock.invoke as unknown as {
      mockImplementation: (f: (cmd: string) => unknown) => void;
    }).mockImplementation((cmd: string) =>
      Promise.resolve(cmd === 'ai_keychain_get' ? 'sk-restored' : undefined)
    );

    await useSettingsStore.getState().retryKeychainPermission();

    expect(useSettingsStore.getState().settings.ai.apiKey).toBe('sk-restored');
    expect(useSettingsStore.getState().keychainPermissionPromptOpen).toBe(false);
    expect(useSettingsStore.getState().keychainPermissionRetryError).toBeNull();
  });

  it('再次拒绝时保留提醒并记录可本地化状态', async () => {
    useSettingsStore.setState({ keychainPermissionPromptOpen: true });
    (tauriMock.invoke as unknown as {
      mockImplementation: (f: (cmd: string) => unknown) => void;
    }).mockImplementation((cmd: string) => {
      if (cmd === 'ai_keychain_get') {
        return Promise.reject(Object.assign(new Error('cancelled'), { kind: 'AiKeyringAccessDenied', data: {} }));
      }
      return Promise.resolve(undefined);
    });

    await useSettingsStore.getState().retryKeychainPermission();

    expect(useSettingsStore.getState().keychainPermissionPromptOpen).toBe(true);
    expect(useSettingsStore.getState().keychainPermissionRetrying).toBe(false);
    expect(useSettingsStore.getState().keychainPermissionRetryError).toBe('denied');
  });

  it('稍后处理只在勾选时持久化不再提醒', async () => {
    useSettingsStore.setState({ keychainPermissionPromptOpen: true });
    useSettingsStore.getState().dismissKeychainPermissionPrompt(false);
    expect(useSettingsStore.getState().keychainPermissionPromptOpen).toBe(false);
    expect(
      useSettingsStore.getState().settings.general.suppressKeychainPermissionReminder
    ).toBe(false);

    useSettingsStore.setState({ keychainPermissionPromptOpen: true });
    useSettingsStore.getState().dismissKeychainPermissionPrompt(true);
    expect(
      useSettingsStore.getState().settings.general.suppressKeychainPermissionReminder
    ).toBe(true);

    await vi.waitFor(() => {
      const saved = tauriMock.fileStore.get(STORE_FILE)?.get(STORE_KEY) as AppSettings;
      expect(saved.general.suppressKeychainPermissionReminder).toBe(true);
    });
  });
});
