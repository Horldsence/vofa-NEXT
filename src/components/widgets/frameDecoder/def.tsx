// ============ 帧解码控件定义 ============
import { ScanText } from 'lucide-react';
import { memo } from 'react';
import type { WidgetConfig } from '../../../types';
import { NodePlaceholder } from '../shared/NodePlaceholder';
import type { WidgetDef } from '../registryTypes';

const frameDecoderPlaceholder = memo(function frameDecoderPlaceholder({ widget }: { widget: Extract<WidgetConfig, { kind: 'FrameDecoder' }> }) {
  return <NodePlaceholder kind='FrameDecoder' nodeId={widget.params.id} />;
});

export const frameDecoderDef: WidgetDef<'FrameDecoder'> = {
  kind: 'FrameDecoder',
  icon: ScanText,
  labelKey: 'frameDecoder',
  Component: frameDecoderPlaceholder,
};
