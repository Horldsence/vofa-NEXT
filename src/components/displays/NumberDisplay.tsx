import { Settings2 } from 'lucide-react';
import type { WidgetConfig } from '../../types';
import { useGraphInput } from '../../lib/useGraphInput';

interface NumberDisplayProps {
  widget: Extract<WidgetConfig, { kind: 'NumberDisplay' }>;
  onRemove: () => void;
  onEdit?: () => void;
}

/// 大数字显示 — 大字号展示单通道数值, 含单位与小数位
/// 数据源: edge 连线 (后端图输出) 优先, 否则回退到 channel 参数
export function NumberDisplay({ widget, onEdit }: NumberDisplayProps) {
  const { unit, precision, channel } = widget.params;
  const value = useGraphInput(widget.params.id, 'value', channel, 0);

  // 自适应字号: 值越长字号越小
  const text = value.toFixed(precision);
  const fontSize = text.length > 10 ? 18 : text.length > 7 ? 24 : 32;

  return (
    <div className="group bg-bg-sidebar border border-border rounded p-2.5 min-w-[140px] flex flex-col gap-1.5 relative">
      {onEdit && (
        <button
          className="absolute top-1 opacity-0 transition-opacity duration-150 group-hover:opacity-100 w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary"
          onClick={onEdit}
          title="Edit"
          style={{ right: 24 }}
        >
          <Settings2 size={11} />
        </button>
      )}
      <div className="flex items-baseline justify-center gap-1 px-1 py-3 min-h-[56px]">
        <span className="font-mono font-bold text-text-bright tracking-[-0.5px] leading-none" style={{ fontSize }}>
          {text}
        </span>
        {unit && <span className="text-sm text-text-secondary font-normal">{unit}</span>}
      </div>
      {channel === null && (
        <div className="text-[10px] text-text-secondary text-center opacity-60">未绑定通道</div>
      )}
    </div>
  );
}
