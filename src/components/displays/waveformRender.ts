/// 波形曲线渲染构造器 - 将 SeriesRender (lineMode + pointMode) 转为 uPlot 配置
/// - lineMode: 决定采样点之间的连线方式 (直线/样条/阶梯)
/// - pointMode: 决定数据点标记的绘制样式 (无/实心圆/空心环/方块)
import uPlot from 'uplot';
import {
  DEFAULT_RENDER,
  type LineMode,
  type PointMode,
  type SeriesRender,
} from '../../types';

/// uPlot gap-clip 标志位 (与 uPlot 内部一致: FILL=1, STROKE=2)
const BAND_CLIP_FILL = 1;
const BAND_CLIP_STROKE = 2;

/// 可选 lineMode 列表 (供 UI 下拉使用)
export const LINE_MODE_OPTIONS: LineMode[] = ['linear', 'spline', 'steppedBefore', 'steppedAfter'];
/// 可选 pointMode 列表 (供 UI 下拉使用)
export const POINT_MODE_OPTIONS: PointMode[] = ['none', 'dot', 'ring', 'square'];

/// 根据 lineMode 构建 uPlot 线条路径构造器 (series.paths)
export function buildLinePath(render: SeriesRender): uPlot.Series.PathBuilder {
  const paths = uPlot.paths;
  switch (render.lineMode) {
    case 'spline':
      return (paths.spline ?? paths.linear)!();
    case 'steppedBefore':
      return (paths.stepped ?? paths.linear)!({ align: -1 });
    case 'steppedAfter':
      return (paths.stepped ?? paths.linear)!({ align: 1 });
    case 'linear':
    default:
      return paths.linear!();
  }
}

/// 实心方块点标记的自定义路径构造器 (uPlot 内置仅支持圆形)
/// 复用 uPlot valToPos 做坐标映射, 返回与内置 points() 相同的 Paths 结构
function squarePointsBuilder(): uPlot.Series.Points.PathBuilder {
  return (u, seriesIdx, idx0, idx1, filtIdxs) => {
    const dataX = u.data[0];
    const dataY = u.data[seriesIdx];
    if (!dataX || !dataY) return null;
    const scaleKey = u.series[seriesIdx].scale || 'y';
    const series = u.series[seriesIdx];
    const pxRound = ((series as unknown as { pxRound?: (v: number) => number }).pxRound) ?? Math.round;
    const sizeCss = series.points?.size ?? 4;
    const pxRatio = window.devicePixelRatio || 1;
    const half = (sizeCss / 2) * pxRatio;

    const fill = new Path2D();
    const draw = (pi: number) => {
      const xv = dataX[pi];
      const yv = dataY[pi];
      if (xv == null || yv == null || Number.isNaN(yv)) return;
      const x = pxRound(u.valToPos(xv, 'x', true));
      const y = pxRound(u.valToPos(yv, scaleKey, true));
      fill.rect(x - half, y - half, half * 2, half * 2);
    };
    if (filtIdxs) {
      (filtIdxs as number[]).forEach(draw);
    } else {
      for (let pi = idx0; pi <= idx1; pi++) draw(pi);
    }

    const bbox = (u as unknown as { bbox?: { left: number; top: number; width: number; height: number } }).bbox;
    const clip = new Path2D();
    if (bbox) {
      clip.rect(bbox.left - half * 2, bbox.top - half * 2, bbox.width + half * 4, bbox.height + half * 4);
    }
    return { stroke: fill, fill, clip, flags: BAND_CLIP_FILL | BAND_CLIP_STROKE };
  };
}

/// 根据 pointMode + 曲线颜色 构建 uPlot 点标记配置 (series.points)
export function buildSeriesPoints(render: SeriesRender, color: string): uPlot.Series.Points {
  switch (render.pointMode) {
    case 'dot':
      // 实心圆点: 填充曲线颜色 (width=0 时内置 builder 不描边)
      return { show: true, size: 4, fill: color, stroke: color, width: 0 };
    case 'ring':
      // 空心圆环: 透明填充 + 曲线颜色描边
      return { show: true, size: 5, width: 1.2, fill: 'transparent', stroke: color };
    case 'square':
      // 实心方块: 自定义路径构造器 (内置仅支持圆形)
      return { show: true, size: 4, fill: color, stroke: color, width: 0, paths: squarePointsBuilder() };
    case 'none':
    default:
      return { show: false };
  }
}

/// 规范化渲染配置 (省略字段回退默认值), 供消费侧安全使用
export function normalizeRender(render: SeriesRender | undefined): SeriesRender {
  if (!render) return { ...DEFAULT_RENDER };
  return {
    lineMode: render.lineMode ?? DEFAULT_RENDER.lineMode,
    pointMode: render.pointMode ?? DEFAULT_RENDER.pointMode,
  };
}
