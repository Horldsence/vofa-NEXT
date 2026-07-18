import { useState, useEffect, useRef } from 'react';
import type { WidgetConfig } from '../../types';
import { waveformWindow } from '../../lib/dataBuffer';

interface PieChartProps {
  widget: Extract<WidgetConfig, { kind: 'PieChart' }>;
  onRemove: () => void;
  /// true = DataPanel 全尺寸渲染 (左右双栏, Canvas 铺满); false = WidgetNode 紧凑渲染
  full?: boolean;
}

const COLORS = [
  '#75beff', '#89d185', '#e2c08d', '#f48771',
  '#c586c0', '#4ec9b0', '#dcdcaa', '#9cdcfe',
];

/// 饼图控件 — 实时显示各通道最新值占比
/// full 模式: Canvas 铺满主区 + 图例侧栏 (固定 200px)
/// 紧凑模式: 固定 120x120 Canvas + 下方图例 (节点编辑器内)
export function PieChart({ widget, full = false }: PieChartProps) {
  const { segments, channels } = widget.params;
  const [values, setValues] = useState<number[]>(channels.map(() => 0));
  const canvasRef = useRef<HTMLCanvasElement>(null);
  // full 模式下跟踪容器尺寸以触发重绘
  const [size, setSize] = useState({ w: 0, h: 0 });

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

  // full 模式: ResizeObserver 监听容器尺寸
  useEffect(() => {
    if (!full) return;
    const canvas = canvasRef.current;
    if (!canvas) return;
    const parent = canvas.parentElement;
    if (!parent) return;
    const ro = new ResizeObserver((entries) => {
      const r = entries[0].contentRect;
      setSize({ w: Math.floor(r.width), h: Math.floor(r.height) });
    });
    ro.observe(parent);
    return () => ro.disconnect();
  }, [full]);

  // 绘制饼图
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    let w: number, h: number;
    if (full) {
      if (size.w === 0 || size.h === 0) return;
      w = size.w;
      h = size.h;
      canvas.width = w * dpr;
      canvas.height = h * dpr;
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.scale(dpr, dpr);
    } else {
      const rect = canvas.getBoundingClientRect();
      w = rect.width;
      h = rect.height;
      canvas.width = w * dpr;
      canvas.height = h * dpr;
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.scale(dpr, dpr);
    }

    ctx.clearRect(0, 0, w, h);

    const cx = w / 2;
    const cy = h / 2;
    // full 模式下留出标签边距, 紧凑模式保持原样
    const radius = full ? Math.max(10, Math.min(w, h) / 2 - 24) : Math.max(10, Math.min(w, h) / 2 - 10);

    const total = values.reduce((a, b) => a + Math.max(0, b), 0);
    if (total <= 0) {
      ctx.strokeStyle = '#3c3c3c';
      ctx.beginPath();
      ctx.arc(cx, cy, radius, 0, Math.PI * 2);
      ctx.stroke();
      ctx.fillStyle = '#858585';
      ctx.font = `${full ? 13 : 12}px sans-serif`;
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
  }, [values, full, size]);

  // 图例渲染 (full 模式纵向列表, 紧凑模式横向 wrap)
  const legend = (
    <div className={full ? 'flex flex-col gap-1.5' : 'flex flex-wrap gap-y-1 gap-x-3 text-xs'}>
      {segments.map((seg, i) => (
        <div key={i} className={`flex items-center gap-1.5 ${full ? 'bg-bg-input border border-border rounded-sm px-2 py-1' : ''}`}>
          <span
            className="w-2.5 h-2.5 rounded-sm flex-shrink-0"
            style={{ background: COLORS[i % COLORS.length] }}
          />
          <span className={full ? 'text-text-primary text-xs flex-1 truncate' : ''}>{seg}</span>
          <span className={full ? 'text-text-bright font-mono text-xs' : ''}>
            {values[i]?.toFixed(2) ?? '0'}
          </span>
        </div>
      ))}
    </div>
  );

  if (full) {
    // DataPanel 全尺寸: 左右双栏
    return (
      <div className="group bg-bg-sidebar border border-border rounded flex-1 min-w-0 min-h-0 flex relative overflow-hidden">
        {/* 主区: Canvas 铺满, 居中绘制正方形饼图 */}
        <div className="flex-1 min-w-0 min-h-0 relative bg-bg-editor flex items-center justify-center">
          <canvas
            ref={canvasRef}
            style={{ width: '100%', height: '100%', display: 'block' }}
          />
        </div>
        {/* 侧栏: 图例列表 */}
        <div className="w-[200px] flex-shrink-0 border-l border-border bg-bg-sidebar overflow-y-auto flex flex-col gap-2 p-2.5">
          <div className="text-[10px] text-text-secondary uppercase tracking-wide font-semibold px-1">Legend</div>
          {legend}
        </div>
      </div>
    );
  }

  // 紧凑模式: 节点编辑器内, 保持原竖排布局
  return (
    <div className="group bg-bg-sidebar border border-border rounded p-2.5 min-w-[140px] flex flex-col gap-1.5 relative">
      <div className="flex flex-col items-center gap-2 p-2">
        <canvas ref={canvasRef} style={{ width: 120, height: 120 }} />
        {legend}
      </div>
    </div>
  );
}
