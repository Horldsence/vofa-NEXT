import { beforeEach, describe, expect, it, vi } from 'vitest';
import { tauriMock } from '../../test/setup';
import { DEFAULT_SETTINGS } from '../../settings/defaults';
import { useSettingsStore } from '../settingsStore';

const STORE_FILE = 'settings.json';
const STORE_KEY = 'app';

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
});
