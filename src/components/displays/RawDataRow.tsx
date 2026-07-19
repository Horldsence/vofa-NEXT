import { useState, memo } from 'react';
import { rawDataBuffer, RAWDATA_BYTES_PER_ROW } from '../../lib/dataBuffer';
import { ROW_HEIGHT, GROUP_SIZE, formatTime, byteToHex, byteToAscii, isPrintable, hexColorClass, type HexColorMode } from './rawDataViewHelpers';

export interface RowProps {
  index: number;
  view: 'hex' | 'ascii';
  showTimestamp: boolean;
  showOffset: boolean;
  hexColorMode: HexColorMode;
  isSelected: boolean;
  version: number;
  onMouseDown: (e: React.MouseEvent, index: number) => void;
}

/// 原始数据行 — 从全局 buffer 按索引读取, memo 化避免无关重渲染
/// version 用于在底层数据变化时强制刷新可见行
export const Row = memo(function Row({
  index,
  view,
  showTimestamp,
  showOffset,
  hexColorMode,
  isSelected,
  onMouseDown,
}: RowProps) {
  const line = rawDataBuffer.getLine(index);
  const [hovered, setHovered] = useState<number | null>(null);

  const hexWidth = 22;
  const asciiWidth = view === 'hex' ? 18 : 18;

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
      {showOffset && (
        <span className="text-text-secondary text-xs font-mono min-w-[80px] text-right">
          {line.offset.toString(16).padStart(8, '0').toUpperCase()}
        </span>
      )}
      {view === 'hex' ? (
        <>
          <div className="flex-1 flex gap-0.5">
            {Array.from({ length: RAWDATA_BYTES_PER_ROW }, (_, i) => {
              const b = line.bytes[i];
              const isGroupEnd = (i + 1) % GROUP_SIZE === 0 && i !== RAWDATA_BYTES_PER_ROW - 1;
              const present = i < line.bytes.length;
              return (
                <span
                  key={i}
                  className={`
                    inline-flex items-center justify-center font-mono text-xs rounded-sm cursor-default
                    transition-colors
                    ${present ? hexColorClass(b, hexColorMode) : ''}
                    ${present && hovered === i ? 'bg-bg-active text-text-bright' : ''}
                    ${isGroupEnd ? 'mr-2' : ''}
                  `}
                  style={{ width: hexWidth, height: ROW_HEIGHT - 4 }}
                  onMouseEnter={() => present && setHovered(i)}
                >
                  {present ? byteToHex(b) : ''}
                </span>
              );
            })}
          </div>
          <div className="flex gap-0.5">
            {Array.from({ length: RAWDATA_BYTES_PER_ROW }, (_, i) => {
              const b = line.bytes[i];
              const isGroupEnd = (i + 1) % GROUP_SIZE === 0 && i !== RAWDATA_BYTES_PER_ROW - 1;
              const present = i < line.bytes.length;
              return (
                <span
                  key={i}
                  className={`
                    inline-flex items-center justify-center font-mono text-xs rounded-sm cursor-default
                    transition-colors
                    ${present ? (isPrintable(b) ? 'text-green' : 'text-text-disabled') : ''}
                    ${present && hovered === i ? 'bg-bg-active text-text-bright' : ''}
                    ${isGroupEnd ? 'mr-2' : ''}
                  `}
                  style={{ width: asciiWidth, height: ROW_HEIGHT - 4 }}
                  onMouseEnter={() => present && setHovered(i)}
                >
                  {present ? byteToAscii(b) : ''}
                </span>
              );
            })}
          </div>
        </>
      ) : (
        <div className="flex gap-0.5">
          {Array.from({ length: RAWDATA_BYTES_PER_ROW }, (_, i) => {
            const b = line.bytes[i];
            const isGroupEnd = (i + 1) % GROUP_SIZE === 0 && i !== RAWDATA_BYTES_PER_ROW - 1;
            const present = i < line.bytes.length;
            return (
              <span
                key={i}
                className={`
                  inline-flex items-center justify-center font-mono text-xs rounded-sm cursor-default
                  transition-colors
                  ${present ? (isPrintable(b) ? 'text-green' : 'text-text-disabled') : ''}
                  ${present && hovered === i ? 'bg-bg-active text-text-bright' : ''}
                  ${isGroupEnd ? 'mr-2' : ''}
                `}
                style={{ width: asciiWidth, height: ROW_HEIGHT - 4 }}
                onMouseEnter={() => present && setHovered(i)}
              >
                {present ? byteToAscii(b) : ''}
              </span>
            );
          })}
        </div>
      )}
    </div>
  );
});
