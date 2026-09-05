// ============ 波形控件定义 ============
import { LineChart } from 'lucide-react';
import { memo } from 'react';
import type { WidgetConfig } from '../../../types';
import { NodePlaceholder } from '../shared/NodePlaceholder';
import type { WidgetDef } from '../registryTypes';

const waveformPlaceholder = memo(function waveformPlaceholder({ widget }: { widget: Extract<WidgetConfig, { kind: 'Waveform' }> }) {
  return <NodePlaceholder kind='Waveform' nodeId={widget.params.id} />;
});

export const waveformDef: WidgetDef<'Waveform'> = {
  kind: 'Waveform',
  icon: LineChart,
  labelKey: 'waveform',
  Component: waveformPlaceholder,
};
