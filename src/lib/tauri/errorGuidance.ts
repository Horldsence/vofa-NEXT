//! 节点错误首次引导 — 每种错误类型第一次出现时, 在错误通知后追加排查建议
//!
//! - 按 NodeErrorKind 记忆, localStorage 持久化 (重启后不重复引导)
//! - Unknown 类型无专属引导, 跳过
//! - 遵循 settings.general.showContextualTips 开关 (由调用方传入)

import { t, type Lang } from '../../i18n';
import { NodeErrorKind, parseNodeError, type NodeError } from '../../types/errors';

const STORAGE_KEY = 'vofa-error-guides-shown';

/// 各错误类型对应的 i18n 引导文案 key (Unknown 无引导)
const GUIDE_KEYS: Partial<Record<NodeErrorKind, string>> = {
  [NodeErrorKind.Transport]: 'errorGuideTransport',
  [NodeErrorKind.Protocol]: 'errorGuideProtocol',
  [NodeErrorKind.PortNotFound]: 'errorGuidePortNotFound',
  [NodeErrorKind.PortAlreadyOpen]: 'errorGuidePortAlreadyOpen',
  [NodeErrorKind.PortNotOpen]: 'errorGuidePortNotOpen',
  [NodeErrorKind.Io]: 'errorGuideIo',
  [NodeErrorKind.Config]: 'errorGuideConfig',
  [NodeErrorKind.Serde]: 'errorGuideSerde',
};

function loadShown(): Set<string> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    const arr = raw ? (JSON.parse(raw) as unknown) : [];
    return new Set(Array.isArray(arr) ? (arr as string[]) : []);
  } catch {
    return new Set();
  }
}

function saveShown(shown: Set<string>): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify([...shown]));
  } catch {
    // 存储不可用时引导退化为每次显示, 不影响主流程
  }
}

/// 拼装错误通知文案 — 该错误类型首次出现时追加引导, 否则原样返回
export function withErrorGuidance(lang: Lang, err: NodeError, tipsEnabled: boolean): string {
  if (!tipsEnabled) return err.message;
  const guideKey = GUIDE_KEYS[err.kind];
  if (!guideKey) return err.message;
  const shown = loadShown();
  if (shown.has(err.kind)) return err.message;
  shown.add(err.kind);
  saveShown(shown);
  return `${err.message}\n\n${t(lang, 'errorGuidePrefix')}: ${t(lang, guideKey)}`;
}

/// 节点错误通知文案 — parseNodeError + 首次引导的常用组合
export function nodeErrorText(lang: Lang, e: unknown, tipsEnabled: boolean): string {
  return withErrorGuidance(lang, parseNodeError(e), tipsEnabled);
}
