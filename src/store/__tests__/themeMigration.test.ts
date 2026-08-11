import { beforeEach, describe, expect, it, vi } from 'vitest';
import { tauriMock } from '../../test/setup';
import { DEFAULT_SETTINGS } from '../../settings/defaults';
import { DARK_THEME, THEME_TOKENS } from '../../settings/theme';
import { useSettingsStore } from '../settingsStore';

const STORE_FILE = 'settings.json';
const STORE_KEY = 'app';

describe('settingsStore theme migration', () => {
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

  it('loads a legacy custom theme without error and maps it to the complete token structure', async () => {
    // 旧版本保存的自定义主题: 只含部分 token key (缺少后加入的 key), 值为旧式自定义值
    const legacyTheme = {
      id: 'custom-legacy',
      name: 'Legacy Theme',
      isBuiltIn: false,
      tokens: {
        bgEditor: '#101418',
        accent: '#ff8800',
        textSecondary: '#9da5b1',
      },
    };
    tauriMock.seedFile(STORE_FILE, STORE_KEY, {
      appearance: {
        theme: 'custom-legacy',
        customThemes: [legacyTheme],
      },
    });

    await expect(useSettingsStore.getState().load()).resolves.toBeUndefined();

    const { settings, loaded } = useSettingsStore.getState();
    expect(loaded).toBe(true);
    expect(settings.appearance.theme).toBe('custom-legacy');

    const [migrated] = settings.appearance.customThemes;
    expect(migrated.id).toBe('custom-legacy');
    // 旧值保留
    expect(migrated.tokens.bgEditor).toBe('#101418');
    expect(migrated.tokens.accent).toBe('#ff8800');
    expect(migrated.tokens.textSecondary).toBe('#9da5b1');
    // 新语义结构: 补齐全部 THEME_TOKENS key
    for (const token of THEME_TOKENS) {
      expect(migrated.tokens[token]).toBeDefined();
    }
    // 缺失 key 回退到 DARK_THEME 默认值
    expect(migrated.tokens.textDisabled).toBe(DARK_THEME.tokens.textDisabled);
    expect(migrated.tokens.waveformCursor).toBe(DARK_THEME.tokens.waveformCursor);
    // 返回独立对象, 不改动传入的持久化数据
    expect(legacyTheme.tokens.accent).toBe('#ff8800');
    expect('textDisabled' in legacyTheme.tokens).toBe(false);
  });

  it('writes semantic vars on :root after loading a legacy theme', async () => {
    tauriMock.seedFile(STORE_FILE, STORE_KEY, {
      appearance: {
        theme: 'custom-legacy',
        customThemes: [
          { id: 'custom-legacy', name: 'L', isBuiltIn: false, tokens: { bgEditor: '#101418' } },
        ],
      },
    });

    await useSettingsStore.getState().load();

    const root = document.documentElement;
    // 语义变量以别名写入, 运行时动态解析到原始 token
    expect(root.style.getPropertyValue('--color-bg-surface')).toBe('var(--color-bg-editor)');
    expect(root.style.getPropertyValue('--color-danger')).toBe('var(--color-red)');
    expect(root.style.getPropertyValue('--color-text-muted')).toBe('var(--color-text-disabled)');
    // 原始 token 仍按主题写入
    expect(root.style.getPropertyValue('--color-bg-editor')).toBe('#101418');
  });
});
