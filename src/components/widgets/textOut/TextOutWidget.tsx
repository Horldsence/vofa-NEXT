// ============ 文本下发控件 (TextOut) ============
//
// 纯内容组件 — 卡片 chrome (节点框/端口/删除按钮) 由 WidgetNode 提供;
// 配置 (目标 / 换行 / 限速) 在节点属性面板 (TextOutProperties)。

import { memo } from 'react';
import { Send } from 'lucide-react';
import type { WidgetConfig } from '../../../types';
import { useAppStore } from '../../../store/appStore';
import { api } from '../../../lib/tauri/tauri';
import { t } from '../../../i18n';

interface TextOutWidgetProps {
  widget: Extract<WidgetConfig, { kind: 'TextOut' }>;
}

/// 文本下发控件 (TextOut) — 动态发送回传
///
/// 数据流:
///   图内字符串 → text 输入口 → 后端求值透传写本节点槽位
///   → graph_string_outputs[id].text (通用字符串发布) → 发送 ticker 按 minIntervalMs
///   变化限速发往目标 Transport.tx; Send 按钮强制立即发送一次 (send_text_out_now)。
///
/// 卡片内只保留目标只读提示、待发文本实时预览与手动发送。
export const TextOutWidget = memo(function TextOutWidget({ widget }: TextOutWidgetProps) {
  const { id, targetTransport } = widget.params;
  const lang = useAppStore((s) => s.lang);
  const rfNodes = useAppStore((s) => s.rfNodes);
  // 实时预览: 通用字符串发布视图 (graph 求值与前端提交合并, 键 = node id)
  const preview = useAppStore((s) => s.customTextOutputs[id]?.text ?? '');

  const targetNode = rfNodes.find((n) => n.id === targetTransport);
  const targetLabel =
    targetNode && typeof targetNode.data?.label === 'string' && targetNode.data.label
      ? targetNode.data.label
      : null;

  return (
    <div className="flex flex-col gap-1.5">
      {/* 目标只读提示 (配置在属性面板) */}
      <div
        className={`px-1.5 py-0.5 text-[10px] font-mono truncate rounded-sm ${
          targetLabel ? 'text-text-secondary bg-bg-subtle' : 'text-text-disabled italic'
        }`}
        title={t(lang, 'textOutTarget')}
      >
        → {targetLabel ?? t(lang, 'textOutNoTarget')}
      </div>

      {/* 待发文本实时预览 + 手动发送 */}
      <div className="flex items-stretch gap-1">
        <div
          className="flex-1 px-1.5 py-1 bg-bg-input border border-border rounded-sm text-xs font-mono text-text-primary break-all whitespace-pre-wrap max-h-[60px] overflow-auto"
          title={preview}
        >
          {preview || <span className="text-text-disabled italic">—</span>}
        </div>
        <button
          type="button"
          disabled={!targetTransport}
          onClick={() => void api.sendTextOutNow(id).catch(() => { return undefined; })}
          title={
            targetTransport
              ? t(lang, 'textOutSendNow')
              : t(lang, 'textOutNoTarget')
          }
          className="w-7 flex items-center justify-center rounded-sm bg-bg-button hover:bg-bg-button-hover disabled:opacity-40 text-text-inverse transition-colors"
        >
          <Send size={12} />
        </button>
      </div>
    </div>
  );
});
