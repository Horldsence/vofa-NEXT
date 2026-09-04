// ============ 文本输入 (TextInput) 控件 ============
//
// 节点内文本框, 内容作为参数 text 经 update_tab_graph 同步到后端;
// 后端每帧原样写入字符串平面 out_str[id]["str"] (唯一输出端口 str, string 域),
// 供下游 Str/TextDisplay 等控件连线消费。不做串口发送, 无发送按钮。
// 名称 (label) 与 placeholder 在节点属性面板编辑, 卡片内只留文本输入。

import { memo } from 'react';
import { useAppStore } from '../../store/appStore';
import { WidgetCard } from '../ui/WidgetCard';
import type { WidgetConfig } from '../../types';

interface TextInputProps {
  widget: Extract<WidgetConfig, { kind: 'TextInput' }>;
  onRemove: () => void;
}

/// 文本输入控件 — 受控文本框, 编辑 params.text 写回 store (走既有图同步链路)
export const TextInput = memo(function TextInput({ widget, onRemove }: TextInputProps) {
  const { id, label, text, placeholder } = widget.params;
  const updateWidget = useAppStore((s) => s.updateWidget);

  // 文本变化 → updateWidget → syncTabGraph (后端重编译即生效, 无需额外 IPC)
  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    updateWidget(id, { kind: 'TextInput', params: { ...widget.params, text: e.target.value } });
  };

  return (
    <WidgetCard label={label} onRemove={onRemove}>
      <input
        type="text"
        className="w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded-sm text-xs font-mono focus:outline-none focus:border-accent transition-colors"
        value={text}
        placeholder={placeholder}
        spellCheck={false}
        onChange={handleChange}
      />
    </WidgetCard>
  );
});
