import { memo } from 'react';
import { Send } from 'lucide-react';
import { WidgetCard } from '../../ui/WidgetCard';
import type { WidgetConfig } from '../../../types';
import { useAppStore } from '../../../store/appStore';
import { isGlobalNode } from '../../../store/appStoreHelpers';
import { api } from '../../../lib/tauri/tauri';
import { t } from '../../../i18n';

interface TextOutWidgetProps {
  widget: Extract<WidgetConfig, { kind: 'TextOut' }>;
  onRemove: () => void;
}

const NEWLINE_OPTIONS: { value: 'none' | 'lf' | 'crlf' | 'cr'; key: string }[] = [
  { value: 'none', key: 'textOutNlNone' },
  { value: 'lf', key: 'textOutNlLf' },
  { value: 'crlf', key: 'textOutNlCrlf' },
  { value: 'cr', key: 'textOutNlCr' },
];

/// 文本下发控件 (TextOut) — 动态发送回传
///
/// 数据流:
///   图内字符串 → text 输入口 → 后端求值透传写本节点槽位
///   → graph_string_outputs[id].text (通用字符串发布) → 发送 ticker 按 minIntervalMs
///   变化限速发往目标 Transport.tx; Send 按钮强制立即发送一次 (send_text_out_now)。
///
/// 本组件提供配置 (目标 / 换行 / 间隔)、实时预览与手动发送。
export const TextOutWidget = memo(function TextOutWidget({ widget }: TextOutWidgetProps) {
  const { id, targetTransport, newline, minIntervalMs } = widget.params;
  const lang = useAppStore((s) => s.lang);
  const updateWidget = useAppStore((s) => s.updateWidget);
  const rfNodes = useAppStore((s) => s.rfNodes);
  // 实时预览: 通用字符串发布视图 (graph 求值与前端提交合并, 键 = node id)
  const preview = useAppStore((s) => s.customTextOutputs[id]?.text ?? '');

  const transports = rfNodes.filter((n) => isGlobalNode(n) && n.type === 'transport');

  const patch = (p: Partial<typeof widget.params>) =>
    updateWidget(id, { kind: 'TextOut', params: { ...widget.params, ...p } });

  return (
    <WidgetCard badge={t(lang, 'textOut')} badgeColor="orange">
      <div className="flex flex-col gap-1.5">
        {/* 目标 Transport */}
        <select
          className="form-select"
          value={targetTransport}
          onChange={(e) => patch({ targetTransport: e.target.value })}
          title={t(lang, 'textOutTarget')}
        >
          <option value="">{t(lang, 'textOutNoTarget')}</option>
          {transports.map((n) => {
            const label =
              typeof n.data?.label === 'string' && n.data.label ? n.data.label : n.id;
            return (
              <option key={n.id} value={n.id}>
                {label}
              </option>
            );
          })}
        </select>

        {/* 发送参数行: 换行 + 最小间隔 */}
        <div className="flex items-center gap-1">
          <select
            className="form-select flex-1 min-w-0 text-[10px]"
            value={newline}
            onChange={(e) => patch({ newline: e.target.value as typeof newline })}
            title={t(lang, 'textOutNewline')}
          >
            {NEWLINE_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {t(lang, o.key)}
              </option>
            ))}
          </select>
          <input
            type="number"
            min={0}
            step={10}
            value={minIntervalMs}
            onChange={(e) => {
              const n = parseInt(e.target.value, 10);
              if (!Number.isFinite(n) || n < 0) return;
              patch({ minIntervalMs: n });
            }}
            title={t(lang, 'textOutInterval')}
            className="w-14 px-1 py-0.5 bg-bg-input border border-border rounded-sm text-text-primary text-xs font-mono text-right focus:outline-none focus:border-accent"
          />
          <span className="text-[9px] text-text-secondary flex-shrink-0">ms</span>
        </div>

        {/* 待发文本实时预览 + 手动发送 */}
        <div className="flex items-stretch gap-1 border-t border-dashed border-border pt-1.5 mt-0.5">
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
    </WidgetCard>
  );
});
