import { useState, useEffect, useRef } from 'react';
import type { WidgetConfig } from '../../types';
import { waveformWindow } from '../../lib/dataBuffer';

interface PieChartProps {
  widget: Extract<WidgetConfig, { kind: 'PieChart' }>;
  onRemove: () => void;
}

const COLORS = [
  '#75beff', '#89d185', '#e2c08d', '#f48771',
  '#c586c0', '#4ec9b0', '#dcdcaa', '#9cdcfe',
];

/// 饼图控件 — 实时显示各通道最新值占比
export function PieChart({ widget }: PieChartProps) {
  const { segments, channels } = widget.params;
  const [values, setValues] = useState<number[]>(channels.map(() => 0));
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const interval = setInterval(() => {
      const win = waveformWindow.get();
      const next = channels.map((ch) => {
        const chData = win.channels[ch];
        return chData && chData.length > 0 ? chData[chData.length - 1] : 0;
      });
      setValues(next);
    }, 100);
    return () => clearInterval(interval);
  }, [channels]);

  // 绘制饼图
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
    const cy = h / 2;
    const radius = Math.min(w, h) / 2 - 10;

    ctx.clearRect(0, 0, w, h);

    const total = values.reduce((a, b) => a + Math.max(0, b), 0);
    if (total <= 0) {
      ctx.strokeStyle = '#3c3c3c';
      ctx.beginPath();
      ctx.arc(cx, cy, radius, 0, Math.PI * 2);
      ctx.stroke();
      ctx.fillStyle = '#858585';
      ctx.font = '12px sans-serif';
      ctx.textAlign = 'center';
      ctx.fillText('No data', cx, cy);
      return;
    }

    let startAngle = -Math.PI / 2;
    values.forEach((v, i) => {
      const val = Math.max(0, v);
      if (val <= 0) return;
      const sliceAngle = (val / total) * Math.PI * 2;
      ctx.beginPath();
      ctx.moveTo(cx, cy);
      ctx.arc(cx, cy, radius, startAngle, startAngle + sliceAngle);
      ctx.closePath();
      ctx.fillStyle = COLORS[i % COLORS.length];
      ctx.fill();
      ctx.strokeStyle = '#1e1e1e';
      ctx.lineWidth = 2;
      ctx.stroke();
      startAngle += sliceAngle;
    });
  }, [values]);

  return (
    <div className="widget-card">
      <div className="pie-chart-container">
        <canvas ref={canvasRef} style={{ width: 120, height: 120 }} />
        <div className="pie-chart-legend">
          {segments.map((seg, i) => (
            <div key={i} className="legend-item">
              <span
                className="legend-color"
                style={{ background: COLORS[i % COLORS.length] }}
              />
              <span>{seg}: {values[i]?.toFixed(2) ?? '0'}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
