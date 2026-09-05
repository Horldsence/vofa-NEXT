// ============ 原始数据控件定义 (节点内占位 + 连接状态提示) ============
import { Activity } from 'lucide-react';
import { memo } from 'react';
import type { WidgetConfig } from '../../../types';
import { NodePlaceholder } from '../shared/NodePlaceholder';
import type { WidgetDef } from '../registryTypes';

const rawDataPlaceholder = memo(function rawDataPlaceholder({ widget }: { widget: Extract<WidgetConfig, { kind: 'RawData' }> }) {
  return <NodePlaceholder kind='RawData' nodeId={widget.params.id} showRawDataHint />;
});

export const rawDataDef: WidgetDef<'RawData'> = {
  kind: 'RawData',
  icon: Activity,
  labelKey: 'rawData',
  Component: rawDataPlaceholder,
};
