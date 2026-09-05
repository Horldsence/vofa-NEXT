import { t } from '../../../i18n';
import type { Lang } from '../../../i18n';
import { formatTime } from './rawDataViewHelpers';
import type { PortSampleStatus } from '../../../lib/data/sampleProtocol';
import { useVirtualizer } from '@tanstack/react-virtual';

/// 字符串通道行 (值变化历史) — 与数值行共用同一虚拟列表 (二选一)
export interface RawDataStringRow {
  /// 前端接收时刻 (字符串平面为 latest-value 快照, 无逐次时间戳)
  ts: number;
  text: string;
}

interface Props {
  numRows: { seq: number; ts: number; value: number }[];
  /// 字符串通道行 — 有值时本组件切换为文本行渲染 (numRows 被忽略)
  stringRows?: RawDataStringRow[];
  showTimestamp: boolean;
  lang: Lang;
  grouping: string;
  repr: string;
  channel: string;
  status: PortSampleStatus;
  previewSkipped: number;
  retentionEvicted: number;
  ingressDropped: number;
  error: string | null;
  scrollRef: React.RefObject<HTMLDivElement | null>;
  onScroll: () => void;
}

/// 数值/字符串通道共用的行列表 — 虚拟滚动 + 时间戳。
/// 数值行定高 (20px); 字符串行为多行长文本自适应测量, 原生文本选中/复制。
export function RawDataViewNumericContent({
  numRows,
  stringRows,
  showTimestamp,
  lang,
  status,
  previewSkipped,
  retentionEvicted,
  ingressDropped,
  error,
  scrollRef,
  onScroll,
}: Props) {
  const isString = stringRows !== undefined;
  const rows = stringRows ?? numRows;
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 20,
    overscan: 8,
    // 字符串行高可变 (多行/长文本) — 按实际 DOM 测量; 数值行保持定高
    ...(isString && {
      measureElement: (el: HTMLElement) => el.getBoundingClientRect().height,
    }),
    getItemKey: (index) =>
      isString ? `${rows[index]?.ts ?? 0}-${index}` : numRows[index]?.seq ?? index,
  });
  const labels =
    lang === 'zh'
      ? {
          channel: '通道不存在',
          disconnected: '连接已断开',
          overrun: '数据过载，正在恢复',
          waiting: '等待有效样本…',
          skipped: '预览跳过',
          evicted: '历史淘汰',
          dropped: '采集溢出',
        }
      : {
          channel: 'Channel unavailable',
          disconnected: 'Disconnected',
          overrun: 'Data overrun; recovering',
          waiting: 'Waiting for valid samples…',
          skipped: 'Preview skipped',
          evicted: 'History evicted',
          dropped: 'Ingress dropped',
        };
  const emptyLabel = isString
    ? t(lang, 'rawDataStringEmpty')
    : error ?? (status === 'channel_out_of_range'
        ? labels.channel
        : status === 'disconnected'
          ? labels.disconnected
          : status === 'overrun'
            ? labels.overrun
            : status === 'waiting'
              ? labels.waiting
              : t(lang, 'rawDataEmpty'));
  return (
    <div className="flex-1 flex flex-col min-h-0 overflow-hidden font-mono animate-rawdata-enter select-text">
      {(previewSkipped > 0 || retentionEvicted > 0 || ingressDropped > 0) && (
        <div className="px-2 py-1 text-[10px] text-text-secondary border-b border-border">
          {labels.skipped} {previewSkipped} · {labels.evicted}{' '}
          {retentionEvicted} · {labels.dropped} {ingressDropped}
        </div>
      )}
      <div ref={scrollRef} onScroll={onScroll} className="flex-1 overflow-auto min-h-0">
        {rows.length === 0 ? (
          <div className="flex items-center justify-center h-32 text-text-secondary text-sm">
            {emptyLabel}
          </div>
        ) : (
          <div style={{ height: `${virtualizer.getTotalSize()}px`, position: 'relative' }}>
            {virtualizer.getVirtualItems().map((virtualRow) => {
              if (isString) {
                const s = stringRows[virtualRow.index];
                return (
                  <div
                    key={virtualRow.key}
                    data-index={virtualRow.index}
                    ref={virtualizer.measureElement}
                    className="absolute left-0 top-0 w-full flex items-start gap-2 px-2 text-xs font-mono"
                    style={{ transform: `translateY(${virtualRow.start}px)` }}
                  >
                    {showTimestamp && (
                      <span className="text-accent min-w-[92px] text-right shrink-0">
                        {formatTime(s.ts)}
                      </span>
                    )}
                    <span className="text-text-primary whitespace-pre-wrap break-all min-w-0">
                      {s.text}
                    </span>
                  </div>
                );
              }
              const r = numRows[virtualRow.index];
              return (
                <div
                  key={virtualRow.key}
                  className="absolute left-0 top-0 w-full flex items-center gap-2 px-2 text-xs font-mono"
                  style={{ height: `${virtualRow.size}px`, transform: `translateY(${virtualRow.start}px)` }}
                >
                  {showTimestamp && (
                    <span className="text-accent min-w-[92px] text-right">
                      {formatTime(r.ts)}
                    </span>
                  )}
                  <span className="text-text-primary">
                    {Number.isInteger(r.value)
                      ? r.value.toFixed(0)
                      : r.value.toFixed(4)}
                  </span>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
