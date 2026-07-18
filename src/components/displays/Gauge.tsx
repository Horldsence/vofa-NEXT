import { useEffect, useRef } from 'react';
import { Settings2 } from 'lucide-react';
import type { WidgetConfig } from '../../types';
import { useGraphInput } from '../../lib/useGraphInput';

interface GaugeProps {
  widget: Extract<WidgetConfig, { kind: 'Gauge' }>;
  onRemove: () => void;
  onEdit?: () => void;
}

/// 仪表盘控件 — 半圆指针 + 弧形进度, 显示单通道实时值
/// 数据源: edge 连线 (后端图输出) 优先, 否则回退到 channel 参数
export function Gauge({ widget, onEdit }: GaugeProps) {
  const { min, max, unit, channel } = widget.params;
  const value = useGraphInput(widget.params.id, 'value', channel, min);
  const canvasRef = useRef<HTMLCanvasElement>(null);

  // 绘制半圆仪表盘
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.getBoundingClientRect();
    canvas.width = rect.width * dpr;
    canvas.height = rect.height * dpr;
    ctx.scale(dpr, dpr);

    const w = rect.width;
    const h = rect.height;
    const cx = w / 2;
    const cy = h * 0.85;
    const radius = Math.min(w / 2 - 8, h * 0.75);

    ctx.clearRect(0, 0, w, h);

    // 半圆范围: 180° (左) ~ 0° (右)
    const startAngle = Math.PI;          // 180°
    const endAngle = 0;                 // 0°
    const totalAngle = startAngle - endAngle;

    // 背景弧 (灰色)
    ctx.strokeStyle = '#3c3c3c';
    ctx.lineWidth = 8;
    ctx.lineCap = 'round';
    ctx.beginPath();
    ctx.arc(cx, cy, radius, endAngle, startAngle);
    ctx.stroke();

    // 进度弧 (蓝色)
    const ratio = Math.max(0, Math.min(1, (value - min) / (max - min || 1)));
    const valueAngle = startAngle - ratio * totalAngle;
    ctx.strokeStyle = '#75beff';
    ctx.beginPath();
    ctx.arc(cx, cy, radius, valueAngle, startAngle);
    ctx.stroke();

    // 刻度 (5 段)
    ctx.strokeStyle = '#858585';
    ctx.lineWidth = 1;
    ctx.fillStyle = '#858585';
    ctx.font = '9px sans-serif';
    ctx.textAlign = 'center';
    for (let i = 0; i <= 4; i++) {
      const r = i / 4;
      const a = startAngle - r * totalAngle;
      const x1 = cx + Math.cos(a) * (radius - 12);
      const y1 = cy + Math.sin(a) * (radius - 12);
      const x2 = cx + Math.cos(a) * (radius - 4);
      const y2 = cy + Math.sin(a) * (radius - 4);
      ctx.beginPath();
      ctx.moveTo(x1, y1);
      ctx.lineTo(x2, y2);
      ctx.stroke();

      // 刻度标签
      const tickVal = min + r * (max - min);
      const lx = cx + Math.cos(a) * (radius - 22);
      const ly = cy + Math.sin(a) * (radius - 22) + 3;
      const txt = Math.abs(tickVal) >= 100 ? tickVal.toFixed(0) : tickVal.toFixed(1);
      ctx.fillText(txt, lx, ly);
    }

    // 指针
    ctx.strokeStyle = '#f48771';
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.moveTo(cx, cy);
    ctx.lineTo(
      cx + Math.cos(valueAngle) * (radius - 8),
      cy + Math.sin(valueAngle) * (radius - 8)
    );
    ctx.stroke();

    // 中心圆
    ctx.fillStyle = '#cccccc';
    ctx.beginPath();
    ctx.arc(cx, cy, 4, 0, Math.PI * 2);
    ctx.fill();
  }, [value, min, max]);

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
      <div className="flex flex-col items-center gap-1">
        <canvas ref={canvasRef} style={{ width: '100%', height: 90 }} />
        <div className="font-mono text-lg font-semibold text-text-bright text-center">
          {value.toFixed(2)}
          {unit && <span className="ml-1 text-[10px] text-text-secondary font-normal">{unit}</span>}
        </div>
      </div>
    </div>
  );
}
