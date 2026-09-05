import { memo } from 'react';
import { RotateCcw } from 'lucide-react';
import { useAppStore } from '../../../store/appStore';
import type { WidgetConfig } from '../../../types';
import { t } from '../../../i18n';
import { clampWidgetSize, widgetMinSize } from '../../../lib/utils/widgetSize';
import { OptionalNumberField } from '../../ui/fields';

/// 节点尺寸编辑节 — 宽/高 (空 = 随内容自适应) + 重置按钮;
/// 保存到 rfNode 显式尺寸并随位置持久化到后端 (graph slice setWidgetNodeSize)
export const SizeSection = memo(function SizeSection({ nodeId, kind }: {
  nodeId: string;
  kind: WidgetConfig['kind'];
}) {
  const lang = useAppStore((s) => s.lang);
  const width = useAppStore((s) => s.rfNodes.find((n) => n.id === nodeId)?.width ?? null);
  const height = useAppStore((s) => s.rfNodes.find((n) => n.id === nodeId)?.height ?? null);
  const setWidgetNodeSize = useAppStore((s) => s.setWidgetNodeSize);
  const commitSize = (patch: { width?: number | null; height?: number | null }) => {
    const next = clampWidgetSize(kind, {
      width: patch.width !== undefined ? (patch.width ?? undefined) : (width ?? undefined),
      height: patch.height !== undefined ? (patch.height ?? undefined) : (height ?? undefined),
    });
    setWidgetNodeSize(nodeId, next);
  };
  const limits = widgetMinSize(kind);
  return (
    <section className="mt-3 pt-3 border-t border-border">
      <div className="text-[10px] font-semibold uppercase tracking-wider text-text-secondary mb-2">{t(lang, 'nodeSize')}</div>
      <div className="flex gap-2">
        <OptionalNumberField label={t(lang, 'nodeWidth')} value={width} placeholder={t(lang, 'sizeAuto')}
          onCommit={(w) => commitSize({ width: w })} />
        <OptionalNumberField label={t(lang, 'nodeHeight')} value={height} placeholder={t(lang, 'sizeAuto')}
          onCommit={(h) => commitSize({ height: h })} />
      </div>
      <div className="flex items-center justify-between">
        <span className="text-[10px] text-text-secondary">
          {t(lang, 'nodeSizeMinHint').replace('{w}', String(limits.minW)).replace('{h}', String(limits.minH))}
        </span>
        <button type="button" onClick={() => setWidgetNodeSize(nodeId, {})}
          className="inline-flex items-center gap-1 text-[10px] text-text-secondary hover:text-text-primary rounded px-1.5 py-1 hover:bg-bg-hover transition-colors">
          <RotateCcw size={10} /> {t(lang, 'resetSize')}
        </button>
      </div>
    </section>
  );
});
