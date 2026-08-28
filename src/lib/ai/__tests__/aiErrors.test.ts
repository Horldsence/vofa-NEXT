import { describe, expect, it } from 'vitest';
import { formatAiError, formatAiKindError } from '../aiErrors';
import type { Lang } from '../../../i18n';

const zh: Lang = 'zh';

/// AI 错误本地化 — kind + 结构化字段 → 当前语言文案

describe('formatAiKindError', () => {
  it('MissingApiKey 用 adapter 插值', () => {
    const view = formatAiKindError('AiMissingApiKey', { adapter: 'orcarouter' }, '', zh);
    expect(view.summary).toContain('orcarouter');
    expect(view.summary).toContain('API Key');
    expect(view.detail).toBeUndefined();
  });

  it('ProviderRequest 主行为本地化摘要, 原始描述降级为 detail', () => {
    const raw = 'LLM 请求失败 [orcarouter/openai/gpt-4o-mini]: HTTP 401';
    const view = formatAiKindError(
      'AiProviderRequest',
      { adapter: 'orcarouter', model: 'openai/gpt-4o-mini' },
      raw,
      zh
    );
    expect(view.summary).toContain('orcarouter');
    expect(view.summary).toContain('openai/gpt-4o-mini');
    expect(view.detail).toBe(raw);
  });

  it('MaxToolRounds 插值轮次', () => {
    const view = formatAiKindError('AiMaxToolRounds', { rounds: '8' }, '', zh);
    expect(view.summary).toContain('8');
  });

  it('未知 kind 回退原始描述', () => {
    const view = formatAiKindError('AiWhatever', {}, '原始信息', zh);
    expect(view.summary).toBe('原始信息');
  });
});

describe('formatAiError', () => {
  it('IPC 结构化错误对象 ({kind,message,data}) → 本地化', () => {
    const view = formatAiError(
      { kind: 'AiMissingApiKey', message: 'provider [openai] 缺少 API key', data: { adapter: 'openai' } },
      zh
    );
    expect(view.summary).toContain('openai');
    // 不再把对象序列化成 [object Object] / JSON 串
    expect(view.summary).not.toContain('kind');
  });

  it('普通 Error / 字符串原样透传', () => {
    expect(formatAiError(new Error('boom'), zh).summary).toBe('boom');
    expect(formatAiError('boom', zh).summary).toBe('boom');
  });
});
