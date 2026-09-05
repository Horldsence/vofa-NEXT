// ============ 频谱展示控件 — 节点内占位 ============
import { memo } from 'react';
import type { WidgetConfig } from '../../../types';
import { NodePlaceholder } from '../shared/NodePlaceholder';

/// 纯窗口型展示 — 节点内仅占位提示, 实际绘制在数据窗口
export const SpectrumPlaceholder = memo(function SpectrumPlaceholder({ widget }: { widget: Extract<WidgetConfig, { kind: 'Spectrum' }> }) {
  return <NodePlaceholder kind="Spectrum" nodeId={widget.params.id} />;
});
