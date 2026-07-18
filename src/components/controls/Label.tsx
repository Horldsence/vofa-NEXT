import { useState, useEffect } from 'react';
import { X } from 'lucide-react';
import type { WidgetConfig } from '../../types';
import { waveformWindow } from '../../lib/dataBuffer';

interface LabelProps {
  widget: Extract<WidgetConfig, { kind: 'Label' }>;
  onRemove: () => void;
}

/// 标签控件 — 显示通道实时值或固定文本
export function Label({ widget, onRemove }: LabelProps) {
  const { text, channel } = widget.params;
  const [display, setDisplay] = useState(text);

  useEffect(() => {
    // 无通道绑定: 显示静态文本
    if (channel === null) {
      setDisplay(text);
      return;
    }

    // 绑定通道: 轮询后端波形窗口缓存
    setDisplay(text);
    const interval = setInterval(() => {
      const win = waveformWindow.get();
      const ch = win.channels[channel];
      if (ch && ch.length > 0) {
        const last = ch[ch.length - 1];
        setDisplay(`${text}: ${last.toFixed(3)}`);
      }
    }, 100);

    return () => clearInterval(interval);
  }, [text, channel]);

  return (
    <div className="group bg-bg-sidebar border border-border rounded p-2.5 min-w-[140px] flex flex-col gap-1.5 relative">
      <button
        className="absolute top-1 right-1 opacity-0 transition-opacity duration-150 group-hover:opacity-100 w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary"
        onClick={onRemove}
      >
        <X size={12} />
      </button>
      <div className="text-xs text-text-secondary uppercase tracking-[0.3px]">{channel === null ? 'Label' : `CH${channel}`}</div>
      <div className="text-xl font-semibold text-text-bright font-mono text-center">{display}</div>
    </div>
  );
}
