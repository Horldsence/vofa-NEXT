import { useState, useEffect, useMemo, useRef } from 'react';
import { useAppStore } from '../../store/appStore';
import { logicSampleBuffer } from '../../lib/logicBuffer';
import { clearLogicBuffer } from '../../lib/logicSubscription';
import { t } from '../../i18n';
import { Trash2, ArrowDown } from 'lucide-react';
import type { LogicSample } from '../../types';

/// 逻辑分析仪时序图 — 多通道数字波形 (SVG 阶梯线)
export function LogicTimingChart() {
  const lang = useAppStore((s) => s.lang);
  const logicSamplesVersion = useAppStore((s) => s.logicSamplesVersion);

  const [samples, setSamples] = useState<LogicSample[]>([]);
  const [autoScroll, setAutoScroll] = useState(true);
  const [zoom, setZoom] = useState(1); // 1 = 1px per sample
  const containerRef = useRef<HTMLDivElement>(null);

  // 订阅 logicSampleBuffer
  useEffect(() => {
    const unsub = logicSampleBuffer.subscribe((recent) => {
      setSamples(recent);
    });
    setSamples(logicSampleBuffer.getRecent(1000));
    return unsub;
  }, []);

  // 自动滚动到最右
  useEffect(() => {
    if (autoScroll && containerRef.current) {
      containerRef.current.scrollLeft = containerRef.current.scrollWidth;
    }
  }, [samples, autoScroll, logicSamplesVersion]);

  const handleClear = () => {
    void clearLogicBuffer();
    logicSampleBuffer.clear();
    setSamples([]);
  };

  // 通道数 (从最后一个采样获取)
  const channelCount = samples.length > 0 ? samples[samples.length - 1].channel_count : 8;
  const visibleChannels = Math.min(channelCount, 16); // 最多显示 16 通道

  // SVG 尺寸
  const rowHeight = 32;
  const labelWidth = 50;
  const sampleWidth = zoom; // 每采样像素宽度
  const totalWidth = Math.max(samples.length * sampleWidth, 100);
  const totalHeight = visibleChannels * rowHeight + 20;

  // 为每通道生成 SVG path (阶梯线)
  const channelPaths = useMemo(() => {
    const paths: string[] = [];
    for (let ch = 0; ch < visibleChannels; ch++) {
      if (samples.length === 0) {
        paths.push('');
        continue;
      }
      const yHigh = ch * rowHeight + 6;
      const yLow = ch * rowHeight + rowHeight - 6;
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

  // 通道颜色 (8 通道循环)
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

  return (
    <div className="h-full flex flex-col overflow-hidden">
      {/* 工具栏 */}
      <div className="flex gap-2 p-1 items-center border-b border-border bg-bg-panel-header flex-shrink-0">
        <span className="text-xs text-text-secondary">
          {samples.length} samples / {visibleChannels} ch
        </span>
        <div className="flex-1" />
        <label className="flex items-center gap-1 text-xs text-text-secondary">
          <span>Zoom</span>
          <input
            type="range"
            min={0.5}
            max={10}
            step={0.5}
            value={zoom}
            onChange={(e) => setZoom(parseFloat(e.target.value))}
            className="w-20"
          />
        </label>
        <button
          className={`w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer ${autoScroll ? 'text-text-bright' : ''}`}
          title={t(lang, 'autoScroll')}
          onClick={() => setAutoScroll(!autoScroll)}
        >
          <ArrowDown size={14} />
        </button>
        <button
          className="w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer"
          title={t(lang, 'clear')}
          onClick={handleClear}
        >
          <Trash2 size={14} />
        </button>
      </div>

      {/* 时序图 */}
      <div
        className="flex-1 overflow-auto min-h-0"
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
            {/* 通道标签背景 */}
            <rect x={0} y={0} width={labelWidth} height={totalHeight} fill="var(--color-bg-panel-header)" />
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
            {/* 通道标签 */}
            {Array.from({ length: visibleChannels }).map((_, ch) => (
              <text
                key={`label-${ch}`}
                x={8}
                y={ch * rowHeight + rowHeight / 2 + 4}
                fill={channelColors[ch % channelColors.length]}
                fontSize={11}
                fontFamily="monospace"
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
