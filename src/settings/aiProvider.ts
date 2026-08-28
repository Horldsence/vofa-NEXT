//! AI provider 发送前检查 — 与后端 `validate_config` 同规则的前置拦截。
//!
//! 在发送入口 (面板按钮禁用 + store 防御) 提前拦截配置缺失, 避免
//! 请求发出后才收到后端错误;kind 与后端 `AiError::kind()` 一致,
//! 错误文案复用 `lib/ai/aiErrors` 的同一套 i18n。

import type { AppSettings } from './defaults';

/** 校验结果 — null 表示可发送 */
export interface AiSendIssue {
  /** 错误种类 (AiMissingApiKey / AiMissingBaseUrl / AiMissingModel) */
  kind: string;
  /** 本地化插值参数 */
  params: Record<string, string>;
}

/** 检查 provider 配置是否满足发送条件 */
export function checkAiProviderSettings(ai: AppSettings['ai']): AiSendIssue | null {
  if (ai.adapter !== 'ollama' && !ai.apiKey.trim()) {
    return { kind: 'AiMissingApiKey', params: { adapter: ai.adapter } };
  }
  if (ai.adapter === 'openai_compatible' && !ai.baseUrl.trim()) {
    return { kind: 'AiMissingBaseUrl', params: {} };
  }
  if (!ai.model.trim()) {
    return { kind: 'AiMissingModel', params: { adapter: ai.adapter } };
  }
  return null;
}
