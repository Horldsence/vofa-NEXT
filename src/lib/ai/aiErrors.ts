//! AI 错误本地化 — 把后端结构化错误 (kind + data) 映射为当前语言的用户提示。
//!
//! 两类来源:
//! 1. 命令级失败 (invoke reject):后端 `AppError` 序列化为 `{ kind, message, source?, data }`
//! 2. 流式 Error 事件 / 会话错误条目:`{ message, kind?, data? }` (kind 为 `AppError::kind()`)
//!
//! 无 kind 时回退显示后端原始描述 (历史数据兼容)。

import { t, type Lang } from '../../i18n';

/// 错误展示形态 — summary 为主行,detail 为次要原始信息 (如 provider 返回的 HTTP 错误)
export interface AiErrorView {
  summary: string;
  detail?: string;
}

/** 模板插值 — 与 i18n 约定一致 ({name} 占位) */
function fill(template: string, params: Record<string, string>): string {
  return template.replace(/\{(\w+)\}/g, (m, key: string) => params[key] ?? m);
}

/** 按 kind 本地化 (kind 与后端 error::AiError::kind() 一致) */
export function formatAiKindError(
  kind: string,
  data: Record<string, string> | undefined,
  rawMessage: string,
  lang: Lang
): AiErrorView {
  const params = data ?? {};
  const detail = rawMessage && rawMessage !== '' ? rawMessage : undefined;
  switch (kind) {
    case 'AiMissingApiKey':
      return { summary: fill(t(lang, 'aiErrMissingApiKey'), params) };
    case 'AiMissingBaseUrl':
      return { summary: t(lang, 'aiErrMissingBaseUrl') };
    case 'AiMissingModel':
      return { summary: fill(t(lang, 'aiErrMissingModel'), params) };
    case 'AiUnknownAdapter':
      return { summary: fill(t(lang, 'aiErrUnknownAdapter'), params) };
    case 'AiProviderRequest':
      // 原始描述携带 provider 返回的真实原因 (HTTP 状态 / 限流信息), 作为次要行展示
      return { summary: fill(t(lang, 'aiErrProviderRequest'), params), detail };
    case 'AiMaxToolRounds':
      return { summary: fill(t(lang, 'aiErrMaxToolRounds'), params) };
    case 'AiCancelled':
      return { summary: t(lang, 'aiErrCancelled') };
    case 'AiKeyring':
      return { summary: t(lang, 'aiErrKeyring'), detail };
    case 'AiUnknownSession':
      return { summary: t(lang, 'aiErrUnknownSession') };
    case 'AiPersist':
      return { summary: t(lang, 'aiErrPersist'), detail };
    default:
      // MCP 错误等未映射种类 — 直接展示后端描述
      return { summary: rawMessage || kind };
  }
}

/** 从 IPC reject / 事件负载提取结构化字段 */
export function formatAiError(err: unknown, lang: Lang): AiErrorView {
  if (err && typeof err === 'object' && 'kind' in err) {
    const e = err as { kind: string; message?: string; data?: Record<string, string> };
    return formatAiKindError(e.kind, e.data, e.message ?? '', lang);
  }
  if (err instanceof Error) return { summary: err.message };
  if (typeof err === 'string') return { summary: err };
  try {
    return { summary: JSON.stringify(err) };
  } catch {
    return { summary: String(err) };
  }
}
