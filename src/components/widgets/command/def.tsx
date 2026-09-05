// ============ 命令下发控件定义 ============
import { Send } from 'lucide-react';
import { memo } from 'react';
import type { WidgetConfig } from '../../../types';
import { NodePlaceholder } from '../shared/NodePlaceholder';
import type { WidgetDef } from '../registryTypes';

const commandPlaceholder = memo(function commandPlaceholder({ widget }: { widget: Extract<WidgetConfig, { kind: 'Command' }> }) {
  return <NodePlaceholder kind='Command' nodeId={widget.params.id} />;
});

export const commandDef: WidgetDef<'Command'> = {
  kind: 'Command',
  icon: Send,
  labelKey: 'command',
  Component: commandPlaceholder,
};
