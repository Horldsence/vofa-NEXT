// ============ 文本展示控件 (TextDisplay) ============
//
// 纯内容组件 — 卡片 chrome (节点框/端口/删除按钮) 由 WidgetNode 提供。
//
// 接收字符串端口输入 (与 Trigger 的 text 输出端口连接),
// 在节点内直接渲染。字号/等宽可配置。

import { memo, useMemo } from 'react';
import { useAppStore } from '../../../store/appStore';
import { resolveStringSource } from '../../../lib/utils/stringPorts';
import type { WidgetConfig, TextDisplayConfig } from '../../../types';

interface TextDisplayProps {
  widget: Extract<WidgetConfig, { kind: 'TextDisplay' }>;
}

const FONT_SIZE_CLASS: Record<TextDisplayConfig['fontSize'], string> = {
  sm: 'text-xs',
  base: 'text-sm',
  lg: 'text-base',
};

export const TextDisplay = memo(function TextDisplay({ widget }: TextDisplayProps) {
  const { id, label, fontSize, monospace } = widget.params;
  // 边解析 (仿 useGraphInput 数值版): 有边连到 text 口 → 读上游字符串平面;
  // 无边 → 回退读自己 id (兼容旧图: 旧后端曾直接写 customTextOutputs[id].text)
  const edges = useAppStore((s) => s.rfEdges);
  const src = resolveStringSource(edges, id, 'text');
  const source = src?.source;
  const handle = src?.handle ?? 'text';
  const text = useAppStore((s) =>
    (source ? s.customTextOutputs[source]?.[handle] : s.customTextOutputs[id]?.text) ?? ''
  );

  const wrapperCls = useMemo(
    () =>
      [
        'w-full h-full min-h-[40px] max-h-[200px] p-2 rounded-sm border border-border bg-bg-input text-text-primary overflow-auto break-words whitespace-pre-wrap',
        FONT_SIZE_CLASS[fontSize],
        monospace ? 'font-mono' : '',
      ].join(' '),
    [fontSize, monospace],
  );

  return (
    <div className="flex flex-col gap-1 w-full h-full">
      <div className="text-[10px] text-text-secondary">
        <span className="truncate">{label}</span>
      </div>
      <div className={wrapperCls}>{text || <span className="text-text-disabled italic">(empty)</span>}</div>
    </div>
  );
});
