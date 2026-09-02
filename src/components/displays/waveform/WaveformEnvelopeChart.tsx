import { useEffect, useRef, useState } from 'react';
import { subscribeWaveformEnvelope } from '../../../lib/buffers/envelopeClient';
import type { WaveformEnvelopeFrame } from '../../../lib/data/envelopeProtocol';
import { t } from '../../../i18n';
import { useAppStore } from '../../../store/appStore';

interface WaveformEnvelopeChartProps {
  /// 协议源节点 id (波形缓冲按源订阅)
  sourceId: string;
  /// 通道显示名与颜色 (与主图 series 顺序一致)
  series: { label: string; color: string }[];
}

/**
 * 波形包络图 — 「后端 wgpu 预处理降采样, 前端只显示」原型 (wgpu-prerender
 * 可行性论证的落地形态)。
 *
 * 后端对全量窗口 (≤1M 点) 做逐列 min/max 压缩 (GPU 加速 + CPU 回退), 二进制
 * VENV 帧直达前端; 本组件只做解码 + Canvas2D 缎带绘制 (上限前进 + 下限回折
 * + 半透明填充), count=0 列按断线处理。
 *
 * 原型边界: 无游标/缩放 (与主图互操作留待论证结论采纳后设计), 每通道画一条
 * 包络带; 列中心时间为 x 轴。
 */
export function WaveformEnvelopeChart({ sourceId, series }: WaveformEnvelopeChartProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const latestRef = useRef<WaveformEnvelopeFrame | null>(null);
  const [stats, setStats] = useState<{ n: number; columns: number } | null>(null);
  const lang = useAppStore((s) => s.lang);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    // 列数 = 画布 CSS 宽度 (逐像素列包络), 上限 2048
    const columns = Math.min(2048, Math.max(16, canvas.clientWidth || 800));

    const draw = () => {
      const frame = latestRef.current;
      const ctx = canvas.getContext('2d');
      if (!ctx) return;
      const w = canvas.clientWidth;
      const h = canvas.clientHeight;
      canvas.width = Math.max(1, Math.round(w * dpr));
      canvas.height = Math.max(1, Math.round(h * dpr));
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, w, h);

      // 背景网格 (8×4 示波器分格)
      ctx.strokeStyle = 'rgba(128,128,128,0.15)';
      ctx.lineWidth = 1;
      ctx.beginPath();
      for (let gx = 1; gx < 10; gx++) {
        const x = (w * gx) / 10;
        ctx.moveTo(x, 0);
        ctx.lineTo(x, h);
      }
      for (let gy = 1; gy < 8; gy++) {
        const y = (h * gy) / 8;
        ctx.moveTo(0, y);
        ctx.lineTo(w, y);
      }
      ctx.stroke();

      if (!frame) return;

      // 垂直包络: 全窗口 min/max 并集 → 上下留 5% 边距
      let lo = Number.POSITIVE_INFINITY;
      let hi = Number.NEGATIVE_INFINITY;
      for (const ch of frame.channels) {
        for (let c = 0; c < ch.count.length; c++) {
          if (ch.count[c] === 0) continue;
          if (ch.min[c] < lo) lo = ch.min[c];
          if (ch.max[c] > hi) hi = ch.max[c];
        }
      }
      if (!(hi > lo)) {
        lo = -1;
        hi = 1;
      }
      const pad = (hi - lo) * 0.05;
      lo -= pad;
      hi += pad;
      const toY = (v: number) => h - ((v - lo) / (hi - lo)) * h;
      const cols = frame.columns;
      const toX = (c: number) => (cols <= 1 ? w / 2 : (c / (cols - 1)) * w);

      frame.channels.forEach((ch, idx) => {
        const color = series[idx]?.color ?? '#4a9eff';
        ctx.beginPath();
        let drawing = false;
        // 上限前进
        for (let c = 0; c < cols; c++) {
          if (ch.count[c] === 0) {
            drawing = false;
            continue;
          }
          const x = toX(c);
          const y = toY(ch.max[c]);
          if (!drawing) {
            ctx.moveTo(x, y);
            drawing = true;
          } else {
            ctx.lineTo(x, y);
          }
        }
        // 下限回折 (逆序)
        for (let c = cols - 1; c >= 0; c--) {
          if (ch.count[c] === 0) continue;
          ctx.lineTo(toX(c), toY(ch.min[c]));
        }
        ctx.closePath();
        ctx.fillStyle = `${color}33`; // 半透明填充
        ctx.fill();
        // 中线轮廓 (视觉锚点)
        ctx.beginPath();
        drawing = false;
        for (let c = 0; c < cols; c++) {
          if (ch.count[c] === 0) {
            drawing = false;
            continue;
          }
          const x = toX(c);
          const y = toY((ch.max[c] + ch.min[c]) / 2);
          if (!drawing) {
            ctx.moveTo(x, y);
            drawing = true;
          } else {
            ctx.lineTo(x, y);
          }
        }
        ctx.strokeStyle = color;
        ctx.lineWidth = 1;
        ctx.stroke();
      });
    };

    draw();
    const unsub = subscribeWaveformEnvelope(
      sourceId,
      columns,
      (frame) => {
        latestRef.current = frame;
        setStats({ n: frame.n, columns: frame.columns });
        draw();
      },
      { intervalMs: 33 },
    );
    const onResize = () => draw();
    window.addEventListener('resize', onResize);
    return () => {
      unsub.cancel();
      window.removeEventListener('resize', onResize);
    };
    // columns 依赖画布宽度, 挂载时定一次 (宽度变化由 resize 重绘, 不重建订阅)
  }, [sourceId, series]);

  return (
    <div className="relative w-full h-full">
      <canvas ref={canvasRef} className="w-full h-full block" />
      {/* 图例 + 状态角标 */}
      <div className="absolute top-1.5 left-2 z-10 flex flex-wrap gap-x-3 gap-y-0.5 px-1.5 py-0.5 text-[10px] bg-bg-editor/80 border border-border/30 rounded select-none">
        {series.map((s) => (
          <span key={s.label} className="flex items-center gap-1 font-mono">
            <span className="inline-block w-2 h-2 rounded-sm" style={{ background: s.color }} />
            {s.label}
          </span>
        ))}
      </div>
      {stats && (
        <div className="absolute bottom-1.5 right-2 z-10 px-1.5 py-0.5 text-[10px] text-text-secondary bg-bg-editor/80 border border-border/30 rounded select-none font-mono">
          {t(lang, 'envelopeStats')
            .replace('{n}', stats.n.toLocaleString())
            .replace('{columns}', String(stats.columns))}
        </div>
      )}
    </div>
  );
}
