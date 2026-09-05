// ============ 3D 模型控件定义 (节点内占位, 窗口视图经 DataTabContent 懒加载) ============
import { Box } from 'lucide-react';
import { memo } from 'react';
import type { WidgetConfig } from '../../../types';
import { NodePlaceholder } from '../shared/NodePlaceholder';
import type { WidgetDef } from '../registryTypes';

const model3dPlaceholder = memo(function model3dPlaceholder({ widget }: { widget: Extract<WidgetConfig, { kind: 'Model3D' }> }) {
  return <NodePlaceholder kind='Model3D' nodeId={widget.params.id} />;
});

export const model3dDef: WidgetDef<'Model3D'> = {
  kind: 'Model3D',
  icon: Box,
  labelKey: 'model3d',
  Component: model3dPlaceholder,
};
