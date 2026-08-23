//! 节点错误首次引导 — 每种错误类型第一次出现时, 在错误通知后追加排查建议
//!
//! - 按 NodeErrorKind 记忆, localStorage 持久化 (重启后不重复引导)
//! - Unknown 类型无专属引导, 跳过
//! - 遵循 settings.general.showContextualTips 开关 (内部读取, 调用方不传)

import { t, type Lang } from '../../i18n';
import { NodeErrorKind, parseNodeError, type NodeError } from '../../types/errors';
import { useSettingsStore } from '../../store/settingsStore';

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

/// 当前设置下的引导开关 (settings.general.showContextualTips)
export function tipsEnabled(): boolean {
  return useSettingsStore.getState().settings.general.showContextualTips;
}

/// 节点错误通知文案 — parseNodeError + 首次引导的常用组合
/// (替代各 slice 散落三处的 `function nodeError(lang, e)` 薄封装)
export function nodeErrorText(lang: Lang, e: unknown, tipsEnabled: boolean): string {
  return withErrorGuidance(lang, parseNodeError(e), tipsEnabled);
}

/// 节点错误通知文案 — 默认读取 settings 的引导开关
/// (推荐入口; 三处重复薄封装的统一替代)
export function nodeError(lang: Lang, e: unknown): string {
  return nodeErrorText(lang, e, tipsEnabled());
}

/// 编译/运行诊断归因条目 (按节点) — 用于呈现节点徽标 + toast 摘要
/// (与后端通知事件 struct NodeDiagnostic 对齐, 阶段三错误契约统一)
export interface NodeDiagnostic {
  /// 诊断类型 (编译失败 / 协议节点不存在 / 端口未连接 / 校验错误 / …)
  kind: string;
  /// 归因节点 id (节点不存在等情形可为 null, 表示图级诊断)
  nodeId: string | null;
  /// 文案 (国际化后)
  message: string;
}

/// 通知折叠键 — (kind, nodeId) 替代单一 source, 消除 ×4 同源无归因噪音
/// (前端按此键去重同一节点同一类型诊断的连发通知)
export function diagnosticKey(diag: NodeDiagnostic): string {
  return `${diag.kind}:${diag.nodeId ?? 'global'}`;
}
