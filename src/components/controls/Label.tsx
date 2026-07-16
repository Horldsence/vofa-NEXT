import { useState, useEffect } from 'react';
import { X } from 'lucide-react';
import type { WidgetConfig } from '../../types';
import { waveformBuffer } from '../../lib/dataBuffer';

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

    // 绑定通道: 订阅波形缓冲区更新
    setDisplay(text);
    const interval = setInterval(() => {
      const data = waveformBuffer.getData();
      // data[0] = timestamps, data[i+1] = channel i
      const ch = data[channel + 1];
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
