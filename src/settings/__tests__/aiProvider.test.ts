import { describe, expect, it } from 'vitest';
import { DEFAULT_SETTINGS } from '../../settings/defaults';
import { checkAiProviderSettings } from '../aiProvider';

function aiSettings(overrides: Partial<typeof DEFAULT_SETTINGS.ai> = {}) {
  return { ...DEFAULT_SETTINGS.ai, ...overrides };
}

/// 发送前检查 — 与后端 validate_config 同规则

describe('checkAiProviderSettings', () => {
  it('配置齐全时不拦截', () => {
    expect(
      checkAiProviderSettings(aiSettings({ adapter: 'orcarouter', apiKey: 'sk-orca', model: 'openai/gpt-4o-mini' }))
    ).toBeNull();
  });

  it('非本地 provider 缺 key → AiMissingApiKey (带 adapter 参数)', () => {
    const issue = checkAiProviderSettings(aiSettings({ adapter: 'openai', apiKey: '  ' }));
    expect(issue).toEqual({ kind: 'AiMissingApiKey', params: { adapter: 'openai' } });
  });

  it('ollama 为本地服务, 允许无 key', () => {
    expect(
      checkAiProviderSettings(aiSettings({ adapter: 'ollama', apiKey: '', model: 'qwen3' }))
    ).toBeNull();
  });

  it('openai_compatible 缺端点 → AiMissingBaseUrl', () => {
    const issue = checkAiProviderSettings(aiSettings({ adapter: 'openai_compatible', baseUrl: '', apiKey: 'k' }));
    expect(issue?.kind).toBe('AiMissingBaseUrl');
  });

  it('缺模型名 → AiMissingModel', () => {
    const issue = checkAiProviderSettings(aiSettings({ adapter: 'orcarouter', apiKey: 'sk', model: ' ' }));
    expect(issue).toEqual({ kind: 'AiMissingModel', params: { adapter: 'orcarouter' } });
  });
});
