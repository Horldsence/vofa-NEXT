import { type RefObject } from 'react';
import { formatTimeMs, formatYValue } from './wavechartFormatters';

interface CursorReadoutData {
  leftPx: number;
  topPx: number;
  xSec: number;
  yVal: number;
  yUnit: string;
  channels: { label: string; val: number; color: string; isDerived: boolean }[];
}

interface WaveformCursorReadoutProps {
  readout: CursorReadoutData | null;
  hidden: boolean;
  tooltipPos: { left: number; top: number } | null;
  tooltipRef: RefObject<HTMLDivElement | null>;
}

export function WaveformCursorReadout({
  readout,
  hidden,
  tooltipPos,
  tooltipRef,
}: WaveformCursorReadoutProps) {
  if (!readout || hidden) return null;

  return (
    <div
      ref={tooltipRef}
      className="flex flex-col gap-0.5 px-1.5 py-1 min-w-[80px] bg-bg-editor/95 text-text-primary border border-border/30 rounded font-mono text-xs leading-tight shadow-lg select-none"
      style={{
        position: 'absolute',
        left: tooltipPos ? tooltipPos.left : readout.leftPx + 12,
        top: tooltipPos ? tooltipPos.top : readout.topPx + 12,
        pointerEvents: 'none',
        zIndex: 9,
      }}
    >
      <div className="flex items-center gap-1.5 leading-tight">
        <span className="inline-block min-w-[12px] text-[10px] font-semibold opacity-60 text-center">X</span>
        <span className="font-mono text-xs text-right ml-auto">
          {formatTimeMs(readout.xSec * 1000)}
        </span>
      </div>
      <div className="flex items-center gap-1.5 leading-tight">
        <span className="inline-block min-w-[12px] text-[10px] font-semibold opacity-60 text-center">Y</span>
        <span className="font-mono text-xs text-right ml-auto">
          {formatYValue(readout.yVal, readout.yUnit)}
        </span>
      </div>
      {readout.channels.length > 1 && (
        <div className="h-px my-[3px] bg-border" />
      )}
      {readout.channels.map((ch, i) => (
        <div key={ch.label + i} className="flex items-center gap-1">
          <span
            className="w-2 h-2 rounded-full flex-shrink-0"
            style={{ background: ch.color }}
          />
          <span className="font-semibold opacity-85 min-w-[28px]">{ch.label}</span>
          <span className="ml-auto text-right font-mono text-xs">
            {formatYValue(ch.val, readout.yUnit)}
          </span>
        </div>
      ))}
    </div>
  );
}
