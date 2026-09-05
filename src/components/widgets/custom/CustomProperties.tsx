// ============ 自定义控件属性 ============

import { memo } from 'react';
import { Code2 } from 'lucide-react';
import { useAppStore } from '../../../store/appStore';
import { t } from '../../../i18n';
import type { WidgetPropertiesProps } from '../registryTypes';

/// 自定义 JS 控件属性 — 打开代码编辑器
export const CustomProperties = memo(function CustomProperties({ widget }: WidgetPropertiesProps<'Custom'>) {
  const lang = useAppStore((s) => s.lang);
  const openCustomEditor = useAppStore((s) => s.openCustomEditor);
  return (
    <button type="button" className="w-full h-8 mt-2 bg-bg-button text-text-inverse rounded inline-flex items-center justify-center gap-1.5"
      onClick={() => openCustomEditor(widget.params.id)}><Code2 size={14} /> {t(lang, 'customWidgetEditor')}</button>
  );
});
