// ============ 节点属性面板 (widget 通用壳) ============
//
// 属性面板只负责三件通用事: 名称 / 各 kind 专属编辑节 (registry 分发) / 尺寸。
// 各 kind 的编辑器实现位于 widgets/<kind>/XProperties.tsx, 经 WIDGET_REGISTRY
// 的 Properties 字段挂载 — 新增控件的属性编辑随 def 自动接入本面板。

import { memo } from 'react';
import type { Node } from '@xyflow/react';
import { useAppStore } from '../../store/appStore';
import type { WidgetConfig } from '../../types';
import { t } from '../../i18n';
import { WIDGET_REGISTRY } from '../widgets/registry';
import { TextField } from '../ui/fields';
import { SizeSection } from '../widgets/shared/SizeSection';

export const WidgetProperties = memo(function WidgetProperties({ node }: { node: Node }) {
  const lang = useAppStore((s) => s.lang);
  const widget = useAppStore((s) => s.widgets.find((item) => item.params.id === node.id));
  const updateWidget = useAppStore((s) => s.updateWidget);
  if (!widget) return null;
  const update = (next: WidgetConfig) => updateWidget(widget.params.id, next);
  // def.Properties 是该 kind 的专属编辑节; update 收窄由调用处保证同 kind
  const Properties = WIDGET_REGISTRY[widget.kind].Properties;
  return (
    <>
      <TextField label={t(lang, 'widgetName')} value={widget.params.label}
        onCommit={(label) => update({ ...widget, params: { ...widget.params, label } } as WidgetConfig)} />
      {Properties && <Properties widget={widget as never} update={update} />}
      <SizeSection nodeId={widget.params.id} kind={widget.kind} />
    </>
  );
});
