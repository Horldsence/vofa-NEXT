import { Settings2 } from 'lucide-react';
import type { WidgetConfig } from '../../types';
import { useGraphInput } from '../../lib/useGraphInput';

interface LEDProps {
  widget: Extract<WidgetConfig, { kind: 'LED' }>;
  onRemove: () => void;
  onEdit?: () => void;
}

/// LED 指示灯 — 输入值 >= threshold 显示 ON 颜色, 否则 OFF
/// 数据源: edge 连线 (后端图输出) 优先, 否则回退到 channel 参数
export function LED({ widget, onEdit }: LEDProps) {
  const { threshold, on_color, off_color, channel } = widget.params;
  const value = useGraphInput(widget.params.id, 'value', channel, 0);

  const isOn = value >= threshold;
  const color = isOn ? on_color : off_color;

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
      <div className="flex flex-col items-center gap-1 py-1.5">
        <div
          className={`w-7 h-7 rounded-full border-2 border-[#1e1e1e] transition-[background,box-shadow] duration-100 ${isOn ? 'animate-led-pulse' : ''}`}
          style={{
            background: color,
            boxShadow: isOn ? `0 0 14px ${color}, 0 0 4px ${color}` : 'none',
          }}
        />
        <div className="text-[10px] font-bold text-text-secondary tracking-[0.5px]">{isOn ? 'ON' : 'OFF'}</div>
        <div className="font-mono text-[10px] text-text-secondary">{value.toFixed(3)}</div>
      </div>
    </div>
  );
}
