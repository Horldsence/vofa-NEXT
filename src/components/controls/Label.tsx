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
    <div className="widget-card">
      <button className="btn-icon widget-remove" onClick={onRemove}>
        <X size={12} />
      </button>
      <div className="widget-label">{channel === null ? 'Label' : `CH${channel}`}</div>
      <div className="widget-value">{display}</div>
    </div>
  );
}
