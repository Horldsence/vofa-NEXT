// ============ 饼图属性编辑节 ============
//
// segments 名称列表编辑 — 每段对应一个 segN 输入端口 (数量随 segments 派生)。

import { memo } from 'react';
import { Plus, Trash2 } from 'lucide-react';
import { useAppStore } from '../../../store/appStore';
import { t } from '../../../i18n';
import { TextField } from '../../ui/fields';
import type { WidgetPropertiesProps } from '../registryTypes';

/// 饼图属性 — 分段名称列表 (增 / 删 / 改)
export const PieChartProperties = memo(function PieChartProperties({ widget, update }: WidgetPropertiesProps<'PieChart'>) {
  const lang = useAppStore((s) => s.lang);
  const { segments } = widget.params;
  const patch = (p: Partial<typeof widget.params>) =>
    update({ kind: 'PieChart', params: { ...widget.params, ...p } });
  return (
    <section className="mt-3 pt-3 border-t border-border">
      <div className="flex items-center justify-between mb-2">
        <span className="text-[10px] font-semibold uppercase tracking-wider text-text-secondary">{t(lang, 'pieChartSegments')}</span>
        <button type="button" className="w-6 h-6 flex items-center justify-center rounded hover:bg-bg-hover"
          onClick={() => patch({ segments: [...segments, `Segment ${segments.length + 1}`] })}
          title={t(lang, 'addSegment')} aria-label={t(lang, 'addSegment')}><Plus size={13} /></button>
      </div>
      <div className="flex flex-col gap-2">
        {segments.map((seg, index) => (
          <div key={index} className="flex items-center gap-1">
            <div className="flex-1 min-w-0">
              <TextField label={`${t(lang, 'segmentName')} ${index + 1}`} value={seg}
                onCommit={(next) => patch({ segments: segments.map((item, i) => (i === index ? next : item)) })} />
            </div>
            <button type="button" disabled={segments.length <= 1}
              className="w-6 h-6 self-end mb-2 flex items-center justify-center rounded hover:bg-bg-hover disabled:opacity-40 flex-shrink-0"
              onClick={() => patch({ segments: segments.filter((_, i) => i !== index) })}
              aria-label={`${t(lang, 'segmentName')} ${index + 1}`}><Trash2 size={12} /></button>
          </div>
        ))}
      </div>
    </section>
  );
});
