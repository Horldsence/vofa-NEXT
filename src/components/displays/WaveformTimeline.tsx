import { useRef, useEffect, useCallback } from 'react';
import { waveformWindow } from '../../lib/dataBuffer';
import { TIME_BASES_SEC, formatTimeBase, type ScopeAxisConfig } from '../../types';
import { timeBaseToWindowSec, HORIZONTAL_DIVS } from '../../lib/scopeUtils';
import { CHANNEL_COLORS, TIMELINE_PAD } from './waveformConstants';

/// 命中半径 (px) — 左右柄可点击区域
const HANDLE_HIT_RADIUS = 12;
/// 视觉手柄宽度 (px)
const HANDLE_VISUAL_WIDTH = 8;
/// 视觉手柄最小窗口宽度 (px) — 防止过窄不可见
const MIN_WIN_W_PX = 4;

/// 时间轴缩略图拖动状态
interface DragState {
  active: boolean;
  type: 'window' | 'left' | 'right' | null;
  startX: number;
  startHPos: number;
  startTimeBase: number;
}

/// 悬停状态 (用于视觉反馈)
interface HoverState {
  type: 'window' | 'left' | 'right' | null;
}

interface WaveformTimelineProps {
  axisConfig: ScopeAxisConfig;
  viewEndSec: number;
  timeWindowSec: number;
  connectedChannels: number[];
  /// Stop 模式下的冻结数据快照 — running=false 时使用它绘制缩略图 (而非实时 waveformWindow)
  /// 这样示波器暂停时缩略图也同步冻结, 不会继续显示新到达的数据
  frozenData: { ts: number[]; chs: number[][] } | null;
  onConfigChange?: (next: ScopeAxisConfig) => void;
}

/// 计算可视窗口在缩略图中的几何位置 (canvas 坐标)
/// 绘制与命中检测共享此函数, 确保一致
function computeWindowGeom(
  plotW: number,
  pad: number,
  totalDurSec: number,
  viewEndSec: number,
  timeWindowSec: number
): { winX: number; winW: number; normStart: number; normEnd: number } {
  const winStartSec = viewEndSec - timeWindowSec;
  const normStart = Math.max(0, Math.min(1, (totalDurSec + winStartSec) / totalDurSec));
  const normEnd = Math.max(0, Math.min(1, (totalDurSec + viewEndSec) / totalDurSec));
  const winX = pad + normStart * plotW;
  const winW = Math.max(MIN_WIN_W_PX, (normEnd - normStart) * plotW);
  return { winX, winW, normStart, normEnd };
}

/// 将 timeBase 吸附到最近的 1-2-5 档
function snapTimeBase(tb: number): number {
  let bestIdx = 0, bestDiff = Infinity;
  for (let i = 0; i < TIME_BASES_SEC.length; i++) {
    const diff = Math.abs(TIME_BASES_SEC[i] - tb);
    if (diff < bestDiff) { bestDiff = diff; bestIdx = i; }
  }
  return TIME_BASES_SEC[bestIdx];
}

/// 时间轴缩略图 — 绘制全量波形概览 + 可视窗口框
/// - 拖动窗口体 → 改变 hPosition (查看历史)
/// - 拖动左柄 → 右端点固定, 左端点跟随鼠标 (改变 timeBase)
/// - 拖动右柄 → 左端点固定, 右端点跟随鼠标 (改变 timeBase + hPosition)
/// - 释放时 timeBase 吸附到最近 1-2-5 档
export function WaveformTimeline({
  axisConfig,
  viewEndSec,
  timeWindowSec,
  connectedChannels,
  frozenData,
  onConfigChange,
}: WaveformTimelineProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const dragState = useRef<DragState>({
    active: false, type: null, startX: 0, startHPos: 0, startTimeBase: 0,
  });
  const hoverState = useRef<HoverState>({ type: null });

  const axisConfigRef = useRef(axisConfig);
  useEffect(() => { axisConfigRef.current = axisConfig; }, [axisConfig]);

  // frozenData 用 ref, 避免 draw/hitTest/handleMove 因依赖变化而频繁重建
  const frozenDataRef = useRef(frozenData);
  useEffect(() => { frozenDataRef.current = frozenData; }, [frozenData]);

  /// 获取当前应使用的数据源 — Stop 时用冻结快照, Run 时用实时 waveformWindow
  const getActiveWindow = useCallback(() => {
    const fd = frozenDataRef.current;
    if (fd && fd.ts.length > 0) {
      return {
        timestamps: fd.ts,
        channels: fd.chs,
        channel_count: fd.chs.length,
      };
    }
    return waveformWindow.get();
  }, []);

  // ====== 绘制 ======
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const draw = () => {
      const ctx = canvas.getContext('2d');
      if (!ctx) return;
      const dpr = window.devicePixelRatio || 1;
      const rect = canvas.getBoundingClientRect();
      canvas.width = rect.width * dpr;
      canvas.height = rect.height * dpr;
      ctx.scale(dpr, dpr);

      const w = rect.width;
      const h = rect.height;
      const pad = TIMELINE_PAD;
      const plotW = w - pad * 2;
      const plotH = h - pad * 2;

      ctx.clearRect(0, 0, w, h);
      ctx.fillStyle = '#1e1e1e';
      ctx.fillRect(0, 0, w, h);

      const win = getActiveWindow();
      const totalPoints = win.timestamps.length;
      if (totalPoints < 2) {
        ctx.fillStyle = '#858585';
        ctx.font = '10px sans-serif';
        ctx.textAlign = 'center';
        ctx.fillText('No data', w / 2, h / 2);
        return;
      }

      const firstTs = win.timestamps[0];
      const lastTs = win.timestamps[totalPoints - 1];
      const totalDurMs = lastTs - firstTs;
      if (totalDurMs <= 0) return;
      const totalDurSec = totalDurMs / 1000;

      // 缩略波形
      const chsToDraw = connectedChannels.length > 0
        ? connectedChannels
        : Array.from({ length: win.channel_count }, (_, i) => i);
      let dMin = Infinity, dMax = -Infinity;
      for (const ci of chsToDraw) {
        const ch = win.channels[ci];
        if (!ch) continue;
        for (let i = 0; i < ch.length; i++) {
          const v = ch[i];
          if (isNaN(v)) continue;
          if (v < dMin) dMin = v;
          if (v > dMax) dMax = v;
        }
      }
      if (dMin === Infinity) dMin = -1;
      if (dMax === -Infinity) dMax = 1;
      const dRange = dMax - dMin || 1;

      const step = Math.max(1, Math.floor(totalPoints / plotW));
      for (const ci of chsToDraw) {
        const ch = win.channels[ci];
        if (!ch || ch.length === 0) continue;
        ctx.strokeStyle = CHANNEL_COLORS[ci % CHANNEL_COLORS.length];
        ctx.lineWidth = 0.5;
        ctx.globalAlpha = 0.6;
        ctx.beginPath();
        for (let i = 0; i < totalPoints; i += step) {
          const x = pad + (i / (totalPoints - 1)) * plotW;
          const v = ch[i];
          const y = pad + plotH - ((v - dMin) / dRange) * plotH;
          if (i === 0) ctx.moveTo(x, y);
          else ctx.lineTo(x, y);
        }
        ctx.stroke();
      }
      ctx.globalAlpha = 1;

      // 可视窗口框 (使用共享几何函数)
      const { winX, winW } = computeWindowGeom(
        plotW, pad, totalDurSec, viewEndSec, timeWindowSec
      );

      // 窗口背景
      ctx.fillStyle = 'rgba(117, 190, 255, 0.15)';
      ctx.fillRect(winX, pad, winW, plotH);
      ctx.strokeStyle = '#75beff';
      ctx.lineWidth = 1;
      ctx.strokeRect(winX, pad, winW, plotH);

      // 左右拖动柄 (加粗, 8px 宽, 易抓取; 悬停高亮)
      const hoverType = hoverState.current.type;
      const drawHandle = (x: number, hovered: boolean) => {
        const half = HANDLE_VISUAL_WIDTH / 2;
        ctx.fillStyle = hovered ? '#a8d4ff' : '#75beff';
        ctx.fillRect(x - half, pad, HANDLE_VISUAL_WIDTH, plotH);
        // 悬停时加边框
        if (hovered) {
          ctx.strokeStyle = '#ffffff';
          ctx.lineWidth = 0.5;
          ctx.strokeRect(x - half, pad, HANDLE_VISUAL_WIDTH, plotH);
        }
      };
      drawHandle(winX, hoverType === 'left');
      drawHandle(winX + winW, hoverType === 'right');

      // 标签
      const winEndSec = viewEndSec;
      const winStartSec = winEndSec - timeWindowSec;
      ctx.fillStyle = '#858585';
      ctx.font = '9px sans-serif';
      ctx.textAlign = 'left';
      ctx.fillText(winStartSec.toFixed(2) + 's', pad, h - 2);
      ctx.textAlign = 'right';
      ctx.fillText(winEndSec.toFixed(2) + 's', w - pad, h - 2);
    };

    let rafId: number | null = null;
    const tick = () => { draw(); rafId = requestAnimationFrame(tick); };
    rafId = requestAnimationFrame(tick);
    return () => { if (rafId !== null) cancelAnimationFrame(rafId); };
  }, [viewEndSec, timeWindowSec, connectedChannels]);

  // ====== 命中检测: 判定鼠标位置对应的拖动类型 ======
  const hitTest = useCallback((clientX: number): 'left' | 'right' | 'window' | null => {
    const canvas = canvasRef.current;
    if (!canvas) return null;
    const rect = canvas.getBoundingClientRect();
    const x = clientX - rect.left;
    const w = rect.width;
    const pad = TIMELINE_PAD;
    const plotW = w - pad * 2;

    const win = getActiveWindow();
    const totalPoints = win.timestamps.length;
    if (totalPoints < 2) return null;
    const firstTs = win.timestamps[0];
    const lastTs = win.timestamps[totalPoints - 1];
    const totalDurSec = (lastTs - firstTs) / 1000;
    if (totalDurSec <= 0) return null;

    const cfg = axisConfigRef.current;
    const vEnd = cfg.running ? 0 : -cfg.hPosition;
    const vWin = timeBaseToWindowSec(cfg.timeBase);
    const { winX, winW } = computeWindowGeom(plotW, pad, totalDurSec, vEnd, vWin);

    const hitR = HANDLE_HIT_RADIUS;
    // 优先检测手柄 (避免窗口体覆盖手柄命中区)
    if (x >= winX - hitR && x <= winX + hitR) return 'left';
    if (x >= winX + winW - hitR && x <= winX + winW + hitR) return 'right';
    if (x > winX && x < winX + winW) return 'window';
    return null;
  }, [getActiveWindow]);

  // ====== 鼠标按下: 启动拖动 ======
  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();  // 防止文本选择/浏览器默认拖动
    e.stopPropagation();
    const hit = hitTest(e.clientX);
    if (!hit) return;
    const cfg = axisConfigRef.current;
    dragState.current = {
      active: true,
      type: hit,
      startX: e.clientX,
      startHPos: cfg.hPosition,
      startTimeBase: cfg.timeBase,
    };
  }, [hitTest]);

  // ====== 鼠标移动 (悬停检测, 仅在未拖动时) ======
  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    if (dragState.current.active) return;
    const hit = hitTest(e.clientX);
    const prev = hoverState.current.type;
    if (prev !== hit) {
      hoverState.current.type = hit;
      // 直接设置 cursor, 避免触发 re-render
      const canvas = canvasRef.current;
      if (canvas) {
        canvas.style.cursor =
          hit === 'left' || hit === 'right' ? 'ew-resize'
          : hit === 'window' ? 'grab'
          : 'default';
      }
    }
  }, [hitTest]);

  // ====== 鼠标离开: 清除悬停 ======
  const handleMouseLeave = useCallback(() => {
    if (!dragState.current.active) {
      hoverState.current.type = null;
      const canvas = canvasRef.current;
      if (canvas) canvas.style.cursor = 'default';
    }
  }, []);

  // ====== 全局鼠标移动 + 抬起 (挂在 window, 拖动时生效) ======
  useEffect(() => {
    const handleMove = (e: MouseEvent) => {
      const ds = dragState.current;
      if (!ds.active || !ds.type) return;
      const canvas = canvasRef.current;
      if (!canvas) return;
      const w = canvas.getBoundingClientRect().width;
      const pad = TIMELINE_PAD;
      const plotW = w - pad * 2;

      const win = getActiveWindow();
      const totalPoints = win.timestamps.length;
      if (totalPoints < 2) return;
      const firstTs = win.timestamps[0];
      const lastTs = win.timestamps[totalPoints - 1];
      const totalDurSec = (lastTs - firstTs) / 1000;
      if (totalDurSec <= 0) return;

      const dx = e.clientX - ds.startX;
      const dxSec = (dx / plotW) * totalDurSec;
      const cfg = axisConfigRef.current;

      if (ds.type === 'window') {
        // 拖动窗口体 → 窗口跟随鼠标平移 (仅改 hPosition, timeBase 不变)
        // hPosition >= 0 (0=实时, 正数=查看历史); 向右拖→窗口右移→接近最新→hPosition 减小
        const newHPos = ds.startHPos - dxSec;
        const clamped = Math.max(0, Math.min(totalDurSec, newHPos));
        onConfigChange?.({ ...cfg, hPosition: clamped, running: false });
      } else {
        // 左右柄 → 改变窗口大小 (timeBase)
        // startVEnd: 起始右端点 (相对最新数据, 0=最新, 负数=过去)
        // startWinStartSec: 起始左端点
        const startVEnd = -ds.startHPos;
        const startWinSec = ds.startTimeBase * HORIZONTAL_DIVS;
        const startWinStartSec = startVEnd - startWinSec;

        let newVEnd: number;
        let newWinSec: number;
        if (ds.type === 'left') {
          // 拖左端点: 右端点固定 (viewEndSec 不变), 新窗口宽度 = startWinSec - dxSec
          newVEnd = startVEnd;
          newWinSec = startWinSec - dxSec;
        } else {
          // 拖右端点: 左端点固定, 右端点跟随鼠标 (viewEndSec 改变)
          newVEnd = startVEnd + dxSec;
          newWinSec = newVEnd - startWinStartSec;
        }
        // 限制窗口大小在时基档位范围内
        const minWinSec = TIME_BASES_SEC[0] * HORIZONTAL_DIVS;
        const maxWinSec = TIME_BASES_SEC[TIME_BASES_SEC.length - 1] * HORIZONTAL_DIVS;
        newWinSec = Math.max(minWinSec, Math.min(maxWinSec, newWinSec));
        // 限制 viewEndSec 在数据范围内 [-totalDurSec, 0]
        newVEnd = Math.max(-totalDurSec, Math.min(0, newVEnd));
        const newTimeBase = newWinSec / HORIZONTAL_DIVS;
        onConfigChange?.({
          ...cfg,
          timeBase: newTimeBase,
          hPosition: -newVEnd,
          running: false,
        });
      }
    };

    const handleUp = () => {
      const ds = dragState.current;
      if (ds.active && (ds.type === 'left' || ds.type === 'right')) {
        // 释放时吸附到最近 1-2-5 档
        const cfg = axisConfigRef.current;
        const snapped = snapTimeBase(cfg.timeBase);
        if (snapped !== cfg.timeBase) {
          onConfigChange?.({ ...cfg, timeBase: snapped });
        }
      }
      dragState.current = {
        active: false, type: null, startX: 0, startHPos: 0, startTimeBase: 0,
      };
      hoverState.current.type = null;
    };

    window.addEventListener('mousemove', handleMove);
    window.addEventListener('mouseup', handleUp);
    return () => {
      window.removeEventListener('mousemove', handleMove);
      window.removeEventListener('mouseup', handleUp);
    };
  }, [onConfigChange, getActiveWindow]);

  const timeWindowLabel = formatTimeBase(axisConfig.timeBase).replace('/div', '') + ' ×10';
  const statusLabel = axisConfig.running
    ? 'LIVE'
    : axisConfig.hPosition === 0 ? 'STOP' : axisConfig.hPosition.toFixed(2) + 's';

  return (
    <div className="border-t border-border bg-bg-editor flex-shrink-0">
      <div className="flex items-center gap-0.5 px-1.5 py-0.5 bg-bg-panel-header border-b border-border">
        <span className="text-[10px] text-text-primary font-mono px-1">{timeWindowLabel}</span>
        <div className="flex-1" />
        <span className="text-[10px] text-text-secondary font-mono px-1">
          {statusLabel}
        </span>
      </div>
      <canvas
        ref={canvasRef}
        className="block w-full h-10"
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseLeave={handleMouseLeave}
        style={{
          cursor: 'default',
          touchAction: 'none',
          pointerEvents: 'auto',
          userSelect: 'none',
          WebkitUserSelect: 'none',
        }}
      />
    </div>
  );
}
