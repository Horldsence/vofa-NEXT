// ============ 触发器控件定义 ============
import { Zap } from 'lucide-react';
import { memo } from 'react';
import type { WidgetConfig } from '../../../types';
import { NodePlaceholder } from '../shared/NodePlaceholder';
import type { WidgetDef } from '../registryTypes';

const triggerPlaceholder = memo(function triggerPlaceholder({ widget }: { widget: Extract<WidgetConfig, { kind: 'Trigger' }> }) {
  return <NodePlaceholder kind='Trigger' nodeId={widget.params.id} />;
});

export const triggerDef: WidgetDef<'Trigger'> = {
  kind: 'Trigger',
  icon: Zap,
  labelKey: 'trigger',
  Component: triggerPlaceholder,
};
