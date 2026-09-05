// ============ 表格控件定义 ============
import { Table } from 'lucide-react';
import { memo } from 'react';
import type { WidgetConfig } from '../../../types';
import { NodePlaceholder } from '../shared/NodePlaceholder';
import type { WidgetDef } from '../registryTypes';

const tableViewPlaceholder = memo(function tableViewPlaceholder({ widget }: { widget: Extract<WidgetConfig, { kind: 'TableView' }> }) {
  return <NodePlaceholder kind='TableView' nodeId={widget.params.id} />;
});

export const tableViewDef: WidgetDef<'TableView'> = {
  kind: 'TableView',
  icon: Table,
  labelKey: 'tableViewLabel',
  Component: tableViewPlaceholder,
};
