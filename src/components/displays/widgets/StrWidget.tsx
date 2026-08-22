import { memo } from 'react';
import { Plus } from 'lucide-react';
import { WidgetCard } from '../../ui/WidgetCard';
import type { WidgetConfig } from '../../../types';
import { STR_OP_PORTS } from '../../../types';
import { useAppStore } from '../../../store/appStore';
import { useGraphInputs, useStringInputs } from '../../../lib/hooks/useGraphInput';
import { isPortConnected } from '../../../lib/utils/stringPorts';
import { t } from '../../../i18n';

interface StrWidgetProps {
  widget: Extract<WidgetConfig, { kind: 'Str' }>;
  onRemove: () => void;
}

/// op → i18n label key (strLen / strFind / ...); op 均为全小写单词
function opLabelKey(op: string): string {
  return `str${op.charAt(0).toUpperCase()}${op.slice(1)}`;
}

/// 内联数值端口 → i18n label key (端口 id 与 StrConfig 字段同名: pos/len/size)
const INLINE_PORT_LABEL_KEY: Record<string, string> = {
  pos: 'strPortPos',
  len: 'strPortLen',
  size: 'strPortSize',
};

/// 字符串操作控件 — 显示后端图评估的字符串操作结果
///
/// 数据流 (后端评估, 60 FPS 推送):
///   1. 后端 CompiledGraph 按拓扑序评估: 字符串端口经字符串平面取上游文本,
///      数值端口已连接取上游值 / 未连接用 StrConfig 的 pos/len/size 内联回退,
///      调用 StrOp::evaluate → 写入输出端口 "result" (string → customTextOutputs, time → graphOutputs)
///   2. 后端每 16ms 将输出快照推送至前端
///   3. 本组件按 STR_OP_PORTS[op].outputDomain 读对应平面显示结果
///
/// 数值内联框 (inlineNumPorts):
///   - 端口未连接 → 框启用, 编辑写回 StrConfig 对应字段 (updateWidget → syncTabGraph 触发后端重编译)
///   - 端口已连接 → 框禁用, 展示上游实时值
export const StrWidget = memo(function StrWidget({ widget }: StrWidgetProps) {
  const { id, op } = widget.params;
  const meta = STR_OP_PORTS[op];
  const lang = useAppStore((s) => s.lang);
  const updateWidget = useAppStore((s) => s.updateWidget);
  const edges = useAppStore((s) => s.rfEdges);

  const isStringOut = meta.outputDomain === 'string';
  // 结果预览: string 域读字符串平面, time 域读数值平面 (窄订阅, 只取本节点 result 口)
  const strResult = useAppStore((s) => (isStringOut ? s.customTextOutputs[id]?.result ?? '' : ''));
  const numResult = useAppStore((s) => (isStringOut ? 0 : s.graphOutputs[id]?.result ?? 0));

  const strPorts = meta.inputs.filter((p) => p.domain === 'string');
  const numPorts = meta.inputs.filter((p) => p.domain === 'time');

  // 字符串端口当前值 (边解析读上游); 数值端口上游实时值 (内联框禁用时展示)
  const strInputs = useStringInputs(id, strPorts.map((p) => p.id));
  const numInputs = useGraphInputs(id, numPorts.map((p) => p.id), 0);

  // 数值端口已连接集合 — 内联框启用/禁用判定
  const numConnected = new Map(numPorts.map((p) => [p.id, isPortConnected(edges, id, p.id)]));
  const anyConnected =
    meta.inputs.some((p) => isPortConnected(edges, id, p.id));

  /// 内联框编辑: 端口 id 与 StrConfig 字段同名 (pos/len/size), 写回后经 updateWidget 同步图
  const handleInlineChange = (portId: 'pos' | 'len' | 'size', raw: string) => {
    const n = parseFloat(raw);
    if (!Number.isFinite(n) || n < 0) return;
    updateWidget(id, { kind: 'Str', params: { ...widget.params, [portId]: n } });
  };

  return (
    <WidgetCard badge={t(lang, opLabelKey(op))} badgeColor="orange">
      <div className="flex flex-col gap-1.5">
        {/* 结果预览 */}
        {isStringOut ? (
          <div className="px-1.5 py-1 bg-bg-input border border-border rounded-sm text-xs font-mono text-text-primary break-all whitespace-pre-wrap max-h-[60px] overflow-auto">
            {strResult || <span className="text-text-disabled italic">(empty)</span>}
          </div>
        ) : (
          <div className="flex items-baseline gap-1 justify-center py-1.5">
            <span className="text-[22px] font-semibold text-text-primary font-mono tracking-[-0.5px]">
              {numResult}
            </span>
          </div>
        )}

        {!anyConnected && (
          <div className="flex items-center gap-1 justify-center p-1 text-[10px] text-text-secondary opacity-70">
            <Plus size={10} />
            <span>连接输入</span>
          </div>
        )}

        {/* 输入端口当前值: 字符串端口纯展示, inlineNumPorts 数值端口渲染内联框 */}
        <div className="flex flex-col gap-0.5 border-t border-dashed border-border pt-1 mt-0.5">
          {strPorts.map((p) => (
            <div key={p.id} className="flex justify-between items-center gap-1 text-[10px] px-0.5 py-px">
              <span className="text-text-secondary font-mono flex-shrink-0">{p.label}</span>
              <span className="text-text-primary font-mono truncate" title={strInputs[p.id]}>
                {strInputs[p.id] || '—'}
              </span>
            </div>
          ))}
          {numPorts.map((p) => {
            const portId = p.id as 'pos' | 'len' | 'size';
            const connected = numConnected.get(p.id) ?? false;
            return (
              <div key={p.id} className="flex justify-between items-center gap-1 text-[10px] px-0.5 py-px">
                <span className="text-text-secondary font-mono flex-shrink-0">
                  {t(lang, INLINE_PORT_LABEL_KEY[p.id] ?? p.label)}
                </span>
                <input
                  type="number"
                  min={0}
                  step={1}
                  disabled={connected}
                  value={connected ? numInputs[p.id] : widget.params[portId]}
                  onChange={(e) => handleInlineChange(portId, e.target.value)}
                  title={connected ? t(lang, INLINE_PORT_LABEL_KEY[p.id] ?? p.label) : undefined}
                  className="w-16 px-1 py-0.5 bg-bg-input border border-border rounded-sm text-text-primary text-xs font-mono text-right focus:outline-none focus:border-accent disabled:opacity-60 disabled:cursor-default"
                />
              </div>
            );
          })}
        </div>
      </div>
    </WidgetCard>
  );
});
