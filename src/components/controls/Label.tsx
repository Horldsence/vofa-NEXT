import { useState, useEffect } from 'react';
import type { WidgetConfig } from '../../types';
import { waveformWindow } from '../../lib/dataBuffer';
import { WidgetCard } from '../ui/WidgetCard';

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
    <WidgetCard onRemove={onRemove}>
      <div className="text-xs text-text-secondary uppercase tracking-[0.3px]">{channel === null ? 'Label' : `CH${channel}`}</div>
      <div className="text-xl font-semibold text-text-bright font-mono text-center">{display}</div>
    </WidgetCard>
  );
}
