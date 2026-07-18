import { useState, useEffect, useMemo, useRef } from 'react';
import { useAppStore } from '../../store/appStore';
import { logicSampleBuffer } from '../../lib/logicBuffer';
import { clearLogicBuffer } from '../../lib/logicSubscription';
import { t } from '../../i18n';
import { ToolbarIconButton } from '../ui/ToolbarIconButton';
import { Trash2, ArrowDown, ZoomIn, ZoomOut } from 'lucide-react';
import type { LogicSample } from '../../types';

/// 逻辑分析仪时序图 — 多通道数字波形 (SVG 阶梯线)
export function LogicTimingChart() {
  const lang = useAppStore((s) => s.lang);

  const [samples, setSamples] = useState<LogicSample[]>([]);
  const [autoScroll, setAutoScroll] = useState(true);
  const [zoom, setZoom] = useState(1); // 1 = 1px per sample
  const containerRef = useRef<HTMLDivElement>(null);

  // 订阅 logicSampleBuffer (RAF 节流后触发, 单一数据源)
  useEffect(() => {
    const unsub = logicSampleBuffer.subscribe((recent) => setSamples(recent));
    setSamples(logicSampleBuffer.getRecent(1000));
    return unsub;
  }, []);

  // 自动滚动到最右
  useEffect(() => {
    if (autoScroll && containerRef.current) {
      containerRef.current.scrollLeft = containerRef.current.scrollWidth;
    }
  }, [samples, autoScroll]);

  const handleClear = () => {
    void clearLogicBuffer();
    logicSampleBuffer.clear();
    setSamples([]);
  };

  // 通道数 (从最后一个采样获取)
  const channelCount = samples.length > 0 ? samples[samples.length - 1].channel_count : 8;
  const visibleChannels = Math.min(channelCount, 16); // 最多显示 16 通道

  // SVG 尺寸
  const rowHeight = 36;
  const labelWidth = 56;
  const sampleWidth = zoom; // 每采样像素宽度
  const totalWidth = Math.max(samples.length * sampleWidth, 100);
  const totalHeight = visibleChannels * rowHeight + 24;

  // 为每通道生成 SVG path (阶梯线)
  const channelPaths = useMemo(() => {
    const paths: string[] = [];
    for (let ch = 0; ch < visibleChannels; ch++) {
      if (samples.length === 0) {
        paths.push('');
        continue;
      }
      const yHigh = ch * rowHeight + 8;
      const yLow = ch * rowHeight + rowHeight - 8;
      let path = '';
      let prevLevel: boolean | null = null;
      for (let i = 0; i < samples.length; i++) {
        const level = ((samples[i].channels >> ch) & 1) === 1;
        const x = i * sampleWidth;
        const y = level ? yHigh : yLow;
        if (i === 0) {
          path += `M ${x} ${y}`;
        } else {
          if (prevLevel !== null && level !== prevLevel) {
            // 边沿: 先垂直后水平
            path += ` L ${x} ${prevLevel ? yHigh : yLow} L ${x} ${y}`;
          } else {
            path += ` L ${x} ${y}`;
          }
        }
        prevLevel = level;
      }
      // 末尾延伸
      const lastX = samples.length * sampleWidth;
      path += ` L ${lastX} ${prevLevel ? yHigh : yLow}`;
      paths.push(path);
    }
    return paths;
  }, [samples, visibleChannels, sampleWidth, rowHeight]);

  // 通道颜色 (8 通道循环, VSCode 调试器风格)
  const channelColors = [
    '#89d185', // 绿
    '#75beff', // 蓝
    '#f4c041', // 黄
    '#ff8c69', // 橙
    '#c586c0', // 紫
    '#569cd6', // 深蓝
    '#ce9178', // 棕
    '#dcdcaa', // 浅黄
  ];

  // 垂直网格线
  const gridXSteps = 10;
  const gridXInterval = Math.max(Math.floor(samples.length / gridXSteps), 1);

  return (
    <div className="h-full flex flex-col overflow-hidden bg-bg-editor">
      {/* 工具栏 */}
      <div className="flex flex-wrap gap-2 p-2 items-center border-b border-border bg-bg-panel-header flex-shrink-0">
        <div className="flex items-center gap-1.5 text-xs text-text-secondary font-mono">
          <span className="px-1.5 py-0.5 rounded bg-bg-input">{samples.length} samples</span>
          <span className="px-1.5 py-0.5 rounded bg-bg-input">{visibleChannels} ch</span>
        </div>

        <div className="flex-1 min-w-2" />

        <div className="flex items-center gap-2">
          <ToolbarIconButton
            icon={<ZoomOut />}
            title="Zoom out"
            size="sm"
            onClick={() => setZoom((z) => Math.max(0.5, z - 0.5))}
          />
          <input
            type="range"
            min={0.5}
            max={10}
            step={0.5}
            value={zoom}
            onChange={(e) => setZoom(parseFloat(e.target.value))}
            className="w-20 sm:w-28 slider-input"
          />
          <ToolbarIconButton
            icon={<ZoomIn />}
            title="Zoom in"
            size="sm"
            onClick={() => setZoom((z) => Math.min(10, z + 0.5))}
          />
        </div>

        <ToolbarIconButton
          icon={<ArrowDown />}
          active={autoScroll}
          title={t(lang, 'autoScroll')}
          onClick={() => setAutoScroll(!autoScroll)}
        />
        <ToolbarIconButton
          icon={<Trash2 />}
          variant="danger"
          title={t(lang, 'clear')}
          onClick={handleClear}
        />
      </div>

      {/* 时序图 */}
      <div
        className="flex-1 overflow-auto min-h-0 bg-bg-editor"
        ref={containerRef}
      >
        {samples.length === 0 ? (
          <div className="flex items-center justify-center h-full text-text-secondary text-xs">
            {t(lang, 'noLogicSamples')}
          </div>
        ) : (
          <svg
            width={totalWidth + labelWidth}
            height={totalHeight}
            className="block"
            style={{ minWidth: '100%' }}
          >
            {/* 背景网格 */}
            <rect x={0} y={0} width={totalWidth + labelWidth} height={totalHeight} fill="var(--color-bg-editor)" />

            {/* 通道标签背景 */}
            <rect x={0} y={0} width={labelWidth} height={totalHeight} fill="var(--color-bg-panel-header)" />

            {/* 垂直网格线 */}
            {Array.from({ length: Math.floor(samples.length / gridXInterval) }).map((_, i) => {
              const x = labelWidth + i * gridXInterval * sampleWidth;
              return (
                <line
                  key={`v-${i}`}
                  x1={x}
                  y1={0}
                  x2={x}
                  y2={totalHeight}
                  stroke="var(--color-border)"
                  strokeWidth={0.5}
                  opacity={0.5}
                />
              );
            })}

            {/* 通道分隔线 */}
            {Array.from({ length: visibleChannels + 1 }).map((_, i) => (
              <line
                key={`h-${i}`}
                x1={0}
                y1={i * rowHeight}
                x2={totalWidth + labelWidth}
                y2={i * rowHeight}
                stroke="var(--color-border)"
                strokeWidth={0.5}
              />
            ))}

            {/* 通道标签背景条 (交替) */}
            {Array.from({ length: visibleChannels }).map((_, ch) => (
              <rect
                key={`bg-${ch}`}
                x={labelWidth}
                y={ch * rowHeight + 0.5}
                width={totalWidth}
                height={rowHeight - 1}
                fill={ch % 2 === 0 ? 'rgba(255,255,255,0.015)' : 'transparent'}
              />
            ))}

            {/* 通道标签 */}
            {Array.from({ length: visibleChannels }).map((_, ch) => (
              <text
                key={`label-${ch}`}
                x={10}
                y={ch * rowHeight + rowHeight / 2 + 4}
                fill={channelColors[ch % channelColors.length]}
                fontSize={11}
                fontFamily="monospace"
                fontWeight={600}
              >
                CH{ch}
              </text>
            ))}

            {/* 通道波形 */}
            {channelPaths.map((path, ch) => (
              <path
                key={`path-${ch}`}
                d={path}
                stroke={channelColors[ch % channelColors.length]}
                strokeWidth={1.5}
                fill="none"
                transform={`translate(${labelWidth}, 0)`}
              />
            ))}
          </svg>
        )}
      </div>
    </div>
  );
}
