import { useState, memo } from 'react';
import { type RawDataBuffer, RAWDATA_BYTES_PER_ROW } from '../../lib/dataBuffer';
import { ROW_HEIGHT, GROUP_SIZE, formatTime, byteToHex, byteToAscii, isPrintable, hexColorClass, type HexColorMode, type RawDataGrouping, type RawDataRepr } from './rawDataViewHelpers';

export interface RowProps {
  index: number;
  grouping: RawDataGrouping;
  repr: RawDataRepr;
  buffer: RawDataBuffer;
  showTimestamp: boolean;
  showOffset: boolean;
  hexColorMode: HexColorMode;
  isSelected: boolean;
  version: number;
  onMouseDown: (e: React.MouseEvent, index: number) => void;
}

/// 原始数据行 — 从指定 buffer 按索引读取, memo 化避免无关重渲染
/// grouping × repr 四组合: grid+hex / grid+ascii / line+hex / line+ascii
/// version 用于在底层数据变化时强制刷新可见行
export const Row = memo(function Row({
  index,
  grouping,
  repr,
  buffer,
  showTimestamp,
  showOffset,
  hexColorMode,
  isSelected,
  onMouseDown,
}: RowProps) {
  const isLine = grouping === 'line';
  const line = isLine ? buffer.getNewlineLine(index) : buffer.getLine(index);
  const [hovered, setHovered] = useState<number | null>(null);

  const hexWidth = 22;
  const asciiWidth = 18;
  const cellCount = isLine ? line.bytes.length : RAWDATA_BYTES_PER_ROW;

  const renderCell = (i: number, type: 'hex' | 'ascii') => {
    const b = line.bytes[i];
    const isGroupEnd = (i + 1) % GROUP_SIZE === 0 && i !== cellCount - 1;
    const isCompact = isLine && type === 'ascii';
    const present = i < line.bytes.length;
    const width = type === 'hex' ? hexWidth : asciiWidth;
    const text = present ? (type === 'hex' ? byteToHex(b) : byteToAscii(b)) : '';
    const color =
      type === 'hex'
        ? present
          ? hexColorClass(b, hexColorMode)
          : ''
        : present
          ? isPrintable(b)
            ? 'text-green'
            : 'text-text-disabled'
          : '';
    return (
      <span
        key={i}
        className={`
          inline-flex items-center justify-center font-mono text-xs rounded-sm cursor-default
          transition-colors
          ${color}
          ${present && hovered === i ? 'bg-bg-active text-text-bright' : ''}
          ${!isCompact && isGroupEnd ? 'mr-2' : ''}
        `}
        style={isCompact ? { minWidth: 0 } : { width, height: ROW_HEIGHT - 4 }}
        onMouseEnter={() => present && setHovered(i)}
      >
        {present ? text : ''}
      </span>
    );
  };

  return (
    <div
      className={`flex items-center gap-2 px-2 select-none ${isSelected ? 'bg-accent/20' : 'hover:bg-bg-hover'}`}
      style={{ height: ROW_HEIGHT }}
      onMouseDown={(e) => onMouseDown(e, index)}
      onMouseLeave={() => setHovered(null)}
    >
      {showTimestamp && (
        <span className="text-accent text-xs font-mono min-w-[92px] text-right">
          {formatTime(line.timestamp)}
        </span>
      )}
      {showOffset && !isLine && (
        <span className="text-text-secondary text-xs font-mono min-w-[80px] text-right">
          {line.offset.toString(16).padStart(8, '0').toUpperCase()}
        </span>
      )}
      {repr === 'hex' && (
        <div className="flex-1 flex gap-0.5">
          {Array.from({ length: cellCount }, (_, i) => renderCell(i, 'hex'))}
        </div>
      )}
      <div className={`flex ${isLine ? '' : 'gap-0.5'} ${repr === 'ascii' ? 'flex-1' : ''}`}>
        {Array.from({ length: cellCount }, (_, i) => renderCell(i, 'ascii'))}
      </div>
    </div>
  );
});
