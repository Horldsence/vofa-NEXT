import { useVirtualizer } from '@tanstack/react-virtual';
import type { RawDataBuffer } from '../../../lib/buffers/dataBuffer';
import type { Lang } from '../../../i18n';
import type { RawDataGrouping, RawDataRepr, HexColorMode } from './rawDataViewHelpers';
import { ROW_HEIGHT, HeaderBytes } from './rawDataViewHelpers';
import { Row } from './RawDataRow';

interface Props {
  modeCount: number;
  grouping: RawDataGrouping;
  repr: RawDataRepr;
  buffer: RawDataBuffer;
  showTimestamp: boolean;
  showOffset: boolean;
  hexColorMode: HexColorMode;
  version: number;
  lang: Lang;
  // selection
  selection: Pick<ReturnType<typeof import('../../../lib/hooks/useSelection').useSelection>, 'isSelected'>;
  onRowMouseDown: (e: React.MouseEvent, index: number) => void;
  // scroll
  parentRef: React.RefObject<HTMLDivElement | null>;
  userScrolledRef: React.MutableRefObject<boolean>;
  isAutoScrollingRef: React.MutableRefObject<boolean>;
  scrollAnimRef: React.MutableRefObject<number | null>;
  onScroll: () => void;
  onKeyDown: (e: React.KeyboardEvent) => void;
}

export function RawDataViewContent({
  modeCount,
  grouping,
  repr,
  buffer,
  showTimestamp,
  showOffset,
  hexColorMode,
  version,
  lang,
  selection,
  onRowMouseDown,
  parentRef,
  onScroll,
  onKeyDown,
}: Props) {
  const virtualizer = useVirtualizer({
    count: modeCount,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 5,
    getItemKey: (index) => index,
  });

  const renderHeader = () => (
    <div className="flex items-center gap-2 px-2 py-1 border-b border-border bg-bg-panel-header select-none h-[24px] flex-shrink-0">
      <span className="min-w-[14px]" />
      {showTimestamp && (
        <span className="text-text-secondary text-xs font-mono min-w-[92px] text-right">
          {lang === 'zh' ? '时间戳' : 'Timestamp'}
        </span>
      )}
      {showOffset && grouping === 'grid' && (
        <span className="text-text-secondary text-xs font-mono min-w-[80px] text-right">Offset</span>
      )}
      {grouping === 'line' ? (
        <>
          <div className="flex-1" />
          {repr === 'hex' && (
            <div className="flex gap-0.5">
              <span className="text-text-secondary text-xs font-mono">{lang === 'zh' ? 'ASCII' : 'ASCII'}</span>
            </div>
          )}
        </>
      ) : repr === 'hex' ? (
        <>
          <div className="flex-1 flex gap-0.5">
            <HeaderBytes width={22} />
          </div>
          <div className="flex gap-0.5">
            <HeaderBytes width={18} />
          </div>
        </>
      ) : (
        <div className="flex gap-0.5">
          <HeaderBytes width={18} />
        </div>
      )}
    </div>
  );

  const virtualItems = virtualizer.getVirtualItems();

  return (
    <div
      key={`${grouping}:${repr}`}
      className="flex-1 flex flex-col min-h-0 overflow-hidden font-mono animate-rawdata-enter"
    >
      {renderHeader()}
      <div
        className="flex-1 overflow-auto min-h-0 outline-none"
        ref={parentRef}
        onScroll={onScroll}
        onKeyDown={onKeyDown}
        tabIndex={0}
      >
        {modeCount === 0 ? (
          <div className="flex items-center justify-center h-32 text-text-secondary text-sm">
            {lang === 'zh' ? '暂无数据' : 'No data'}
          </div>
        ) : (
          <div style={{ height: `${virtualizer.getTotalSize()}px`, position: 'relative' }}>
            <div
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: '100%',
                minWidth: grouping === 'line' ? 'max-content' : undefined,
                transform: `translateY(${virtualItems[0]?.start ?? 0}px)`,
              }}
            >
              {virtualItems.map((virtualRow) => (
                <Row
                  key={virtualRow.key}
                  originalIndex={virtualRow.index}
                  filteredIndex={virtualRow.index}
                  grouping={grouping}
                  repr={repr}
                  buffer={buffer}
                  showTimestamp={showTimestamp}
                  showOffset={showOffset}
                  hexColorMode={hexColorMode}
                  isSelected={selection.isSelected(virtualRow.index)}
                  version={version}
                  onMouseDown={onRowMouseDown}
                />
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
