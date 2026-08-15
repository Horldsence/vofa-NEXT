import { beforeEach, describe, expect, it, vi } from 'vitest';
import { tauriMock } from '../../test/setup';
import { DEFAULT_SETTINGS } from '../../settings/defaults';
import { useSettingsStore } from '../settingsStore';

const STORE_FILE = 'settings.json';
const STORE_KEY = 'app';

/// 提取 set_pipeline_config 的调用参数 (invoke mock 无参数签名, 需显式收窄)
function pipelineConfigCalls(): { config: Record<string, number> }[] {
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
        coalesce_max_msgs: DEFAULT_SETTINGS.performance.coalesceMaxMsgs,
        coalesce_max_bytes_kb: DEFAULT_SETTINGS.performance.coalesceMaxBytesKb,
        max_feed_workers: DEFAULT_SETTINGS.performance.maxFeedWorkers,
        feed_parallel_unit: DEFAULT_SETTINGS.performance.feedParallelUnit,
        min_worker_bytes_kb: DEFAULT_SETTINGS.performance.minWorkerBytesKb,
        max_stream_shards: DEFAULT_SETTINGS.performance.maxStreamShards,
        parse_channel_cap: DEFAULT_SETTINGS.performance.parseChannelCap,
      },
    });
  });

  it('pushes pipeline config immediately when a performance setting is updated', () => {
    useSettingsStore.getState().update('performance', 'maxFeedWorkers', 8);

    const calls = pipelineConfigCalls();
    expect(calls).toHaveLength(1);
    expect(calls[0].config.max_feed_workers).toBe(8);
    expect(useSettingsStore.getState().settings.performance.maxFeedWorkers).toBe(8);
  });

  it('does not push pipeline config when a non-performance setting is updated', () => {
    useSettingsStore.getState().update('notifications', 'duration', 3000);

    expect(pipelineConfigCalls()).toHaveLength(0);
  });
});
