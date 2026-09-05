// ============ 图像控件属性编辑节 ============
//
// 宽/高 (最小 16px) + 像素格式。

import { memo } from 'react';
import type { ImageConfig } from '../../../types';
import { useAppStore } from '../../../store/appStore';
import { t } from '../../../i18n';
import { NumberField, SelectField } from '../../ui/fields';
import type { WidgetPropertiesProps } from '../registryTypes';

/// 图像属性 — 尺寸 + 像素格式
export const ImageProperties = memo(function ImageProperties({ widget, update }: WidgetPropertiesProps<'Image'>) {
  const lang = useAppStore((s) => s.lang);
  const { width, height, format } = widget.params;
  const patch = (p: Partial<typeof widget.params>) =>
    update({ kind: 'Image', params: { ...widget.params, ...p } });
  return (
    <>
      <NumberField label={t(lang, 'imageWidth')} value={width}
        onCommit={(v) => {
          const next = Math.round(v);
          if (next < 16) return false;
          patch({ width: next });
          return true;
        }}
        error={t(lang, 'invalidRange')} />
      <NumberField label={t(lang, 'imageHeight')} value={height}
        onCommit={(v) => {
          const next = Math.round(v);
          if (next < 16) return false;
          patch({ height: next });
          return true;
        }}
        error={t(lang, 'invalidRange')} />
      <SelectField label={t(lang, 'imageFormat')} value={format}
        options={([
          ['rgb888', 'formatRgb888'],
          ['rgb565', 'formatRgb565'],
          ['gray8', 'formatGray8'],
        ] as [ImageConfig['format'], string][]).map(([value, key]) => ({ value, label: t(lang, key) }))}
        onChange={(v) => patch({ format: v as ImageConfig['format'] })} />
    </>
  );
});
