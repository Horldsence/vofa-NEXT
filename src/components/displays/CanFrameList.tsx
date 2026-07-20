import { useState, useEffect, useRef, useMemo, useCallback, memo } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { useAppStore } from '../../store/appStore';
import { canFrameBuffer } from '../../lib/canBuffer';
import { clearCanBuffer } from '../../lib/canSubscription';
import { useSelection } from '../../lib/useSelection';
import { writeTextToClipboard } from '../../lib/clipboard';
import { t } from '../../i18n';
import { ToolbarIconButton } from '../ui/ToolbarIconButton';
import { Trash2, ArrowDown, Filter, Download, Copy, Check, X } from 'lucide-react';
import type { CanFrame } from '../../types';

const ROW_HEIGHT = 24;

/// 格式化微秒时间戳为 HH:MM:SS.mmmuuu
function formatCanTime(us: number): string {
  if (!us) return '--:--:--.------';
  const d = new Date(us / 1000);
  const h = d.getHours().toString().padStart(2, '0');
  const m = d.getMinutes().toString().padStart(2, '0');
  const s = d.getSeconds().toString().padStart(2, '0');
  const ms = d.getMilliseconds().toString().padStart(3, '0');
  const us3 = Math.floor(us % 1000).toString().padStart(3, '0');
  return `${h}:${m}:${s}.${ms}${us3}`;
}

/// 格式化 CAN ID
function formatCanId(id: number, extended: boolean): string {
  return extended
    ? id.toString(16).toUpperCase().padStart(8, '0')
    : id.toString(16).toUpperCase().padStart(3, '0');
}

/// 格式化数据为 HEX 字符串
function formatDataHex(data: number[]): string {
  return data.map((b) => b.toString(16).toUpperCase().padStart(2, '0')).join(' ');
}

/// 将帧列表转换为 CSV 文本
function framesToCsv(frames: CanFrame[]): string {
  const rows = [
    ['Time', 'Dir', 'ID', 'Extended', 'RTR', 'DLC', 'Data'].join(','),
    ...frames.map((f) =>
      [
        formatCanTime(f.timestamp),
        f.direction,
        formatCanId(f.id, f.extended),
        f.extended ? 'X' : '',
        f.rtr ? 'R' : '',
        f.dlc,
        f.rtr ? '' : formatDataHex(f.data),
      ].join(',')
    ),
  ];
  return rows.join('\n');
}

/// 导出帧列表为 CSV 文件
function exportFramesToCsv(frames: CanFrame[]): void {
  const blob = new Blob([framesToCsv(frames)], { type: 'text/csv;charset=utf-8;' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = `can-frames-${new Date().toISOString().replace(/[:.]/g, '-')}.csv`;
  a.click();
  URL.revokeObjectURL(url);
}

interface RowProps {
  index: number;
  frame: CanFrame;
  isSelected: boolean;
  onMouseDown: (e: React.MouseEvent, index: number) => void;
}

/// CAN 帧行 — memo 化避免无关重渲染
const Row = memo(function Row({ index, frame, isSelected, onMouseDown }: RowProps) {
  return (
    <div
      className={`flex items-center text-xs font-mono border-b border-border/30 select-none ${
        isSelected ? 'bg-accent/20' : 'hover:bg-bg-hover/60'
      }`}
      style={{ height: ROW_HEIGHT }}
      onMouseDown={(e) => onMouseDown(e, index)}
    >
      <span className="px-3 py-1 text-accent whitespace-nowrap min-w-[132px]">
        {formatCanTime(frame.timestamp)}
      </span>
      <span className="px-2 py-1 whitespace-nowrap min-w-[56px]">
        <span
          className={`inline-flex items-center gap-1 text-[10px] font-semibold ${
            frame.direction === 'Tx' ? 'text-purple' : 'text-green'
          }`}
        >
          {frame.direction === 'Tx' ? '→ Tx' : '← Rx'}
        </span>
      </span>
      <span className="px-3 py-1 text-text-bright whitespace-nowrap min-w-[76px]">
        0x{formatCanId(frame.id, frame.extended)}
      </span>
      <span className="px-2 py-1 text-center text-text-secondary min-w-[40px]">
        {frame.extended ? 'X' : ''}
      </span>
      <span className="px-2 py-1 text-center text-text-secondary min-w-[40px]">
        {frame.rtr ? 'R' : ''}
      </span>
      <span className="px-2 py-1 text-center text-text-secondary min-w-[40px]">
        {frame.dlc}
      </span>
      <span className="px-3 py-1 text-text-primary flex-1 min-w-0 truncate">
        {frame.rtr ? (
          <span className="text-text-secondary italic">(remote)</span>
        ) : (
          formatDataHex(frame.data)
        )}
      </span>
    </div>
  );
});

/// CAN 帧列表显示 — 虚拟滚动 + 选中复制 + 过滤
export function CanFrameList() {
  const lang = useAppStore((s) => s.lang);

  const [version, setVersion] = useState(0);
  const [autoScroll, setAutoScroll] = useState(true);
  const [filterId, setFilterId] = useState('');
  const [filterDir, setFilterDir] = useState<'all' | 'Rx' | 'Tx'>('all');
  const [copyFeedback, setCopyFeedback] = useState(false);
  const parentRef = useRef<HTMLDivElement>(null);
  const userScrolledRef = useRef(false);
  const isAutoScrollingRef = useRef(false);

  // 订阅 buffer 变化, 用 version 触发重渲染
  useEffect(() => {
    const unsub = canFrameBuffer.subscribe(() => setVersion((v) => v + 1));
    setVersion((v) => v + 1);
    return unsub;
  }, []);

  // 过滤 (在 version 变化时重新计算, 用 getUnsafeRef 避免中间拷贝)
  const filtered = useMemo(() => {
    const all = canFrameBuffer.getUnsafeRef();
    if (filterDir !== 'all' || filterId.trim()) {
      return all.filter((f) => {
        if (filterDir !== 'all' && f.direction !== filterDir) return false;
        if (filterId.trim()) {
          const idLower = filterId.trim().toLowerCase().replace(/^0x/, '');
          const idNum = parseInt(idLower, 16);
          if (!isNaN(idNum) && f.id !== idNum) return false;
        }
        return true;
      });
    }
    // 无过滤条件: 复用内部引用 (filtered 作为虚拟滚动数据源)
    return all;
  }, [filterDir, filterId, version]);

  // Rx/Tx 计数: 从内部引用遍历, 避免两次 getAll 拷贝
  const rxTxCounts = useMemo(() => {
    const all = canFrameBuffer.getUnsafeRef();
    let rx = 0, tx = 0;
    for (let i = 0; i < all.length; i++) {
      if (all[i].direction === 'Rx') rx++;
      else tx++;
    }
    return { rxCount: rx, txCount: tx, total: all.length };
  }, [version]);
  const { rxCount, txCount } = rxTxCounts;

  const virtualizer = useVirtualizer({
    count: filtered.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 10,
  });

  const selection = useSelection(filtered.length);

  // 自动滚动到底部
  useEffect(() => {
    if (!autoScroll || userScrolledRef.current || filtered.length === 0) return;
    isAutoScrollingRef.current = true;
    virtualizer.scrollToIndex(filtered.length - 1, { align: 'end' });
    const t = setTimeout(() => {
      isAutoScrollingRef.current = false;
    }, 50);
    return () => clearTimeout(t);
  }, [filtered.length, autoScroll, version, virtualizer]);

  // 检测用户手动滚动
  const handleScroll = useCallback(() => {
    if (isAutoScrollingRef.current || !parentRef.current) return;
    const el = parentRef.current;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 30;
    userScrolledRef.current = !atBottom;
  }, []);

  const handleClear = () => {
    void clearCanBuffer();
    canFrameBuffer.clear();
    selection.clear();
    userScrolledRef.current = false;
    setVersion((v) => v + 1);
  };

  const copySelected = useCallback(async () => {
    const indices = selection.selectedSorted;
    if (indices.length === 0) return;
    const frames = indices.map((i) => filtered[i]).filter(Boolean) as CanFrame[];
    const text = framesToCsv(frames);
    const ok = await writeTextToClipboard(text);
    if (ok) {
      setCopyFeedback(true);
      setTimeout(() => setCopyFeedback(false), 1200);
    }
  }, [selection.selectedSorted, filtered]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      const target = e.target as HTMLElement;
      if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable) return;
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'a') {
        e.preventDefault();
        selection.selectAll();
      }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'c') {
        e.preventDefault();
        void copySelected();
      }
    },
    [selection, copySelected]
  );

  const handleRowMouseDown = useCallback(
    (e: React.MouseEvent, index: number) => {
      if (e.button !== 0) return;
      selection.handleClick(index, e);
    },
    [selection]
  );

  const virtualItems = virtualizer.getVirtualItems();

  return (
    <div className="h-full flex flex-col overflow-hidden bg-bg-editor">
      {/* 工具栏 */}
      <div className="flex flex-wrap gap-2 p-2 items-center border-b border-border bg-bg-panel-header flex-shrink-0">
        <div className="flex items-center gap-1.5 min-w-0">
          <Filter size={12} className="text-text-secondary flex-shrink-0" />
          <input
            type="text"
            className="w-24 sm:w-28 px-2 py-1 bg-bg-input text-text-primary border border-border rounded text-xs focus:outline-none focus:border-accent font-mono"
            placeholder="ID (HEX)"
            value={filterId}
            onChange={(e) => setFilterId(e.target.value)}
          />
        </div>

        <div className="flex items-center gap-0.5">
          {(['all', 'Rx', 'Tx'] as const).map((d) => (
            <button
              key={d}
              type="button"
              className={`px-2 py-0.5 text-[11px] border rounded cursor-pointer transition-all ${
                filterDir === d
                  ? 'bg-accent border-accent text-text-inverse'
                  : 'bg-bg-input text-text-secondary border-border hover:bg-bg-hover hover:text-text-primary'
              }`}
              onClick={() => setFilterDir(d)}
            >
              {d === 'all' ? t(lang, 'all') : d}
            </button>
          ))}
        </div>

        <div className="flex-1 min-w-2" />

        <div className="flex items-center gap-1.5 text-xs text-text-secondary font-mono">
          <span className="hidden sm:inline">{rxTxCounts.total}</span>
          <span className="px-1.5 py-0.5 rounded bg-green/10 text-green">Rx:{rxCount}</span>
          <span className="px-1.5 py-0.5 rounded bg-purple/10 text-purple">Tx:{txCount}</span>
        </div>

        {selection.selected.size > 0 && (
          <>
            <span className="text-text-secondary text-xs">{selection.selected.size}</span>
            <button
              className={`w-7 h-7 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer ${
                copyFeedback ? 'text-green' : ''
              }`}
              title={t(lang, 'copySelected')}
              onClick={() => void copySelected()}
            >
              {copyFeedback ? <Check size={14} /> : <Copy size={14} />}
            </button>
            <button
              className="w-7 h-7 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer"
              title={t(lang, 'clearSelection')}
              onClick={selection.clear}
            >
              <X size={14} />
            </button>
          </>
        )}

        <ToolbarIconButton
          icon={<ArrowDown />}
          active={autoScroll && !userScrolledRef.current}
          title={t(lang, 'autoScroll')}
          onClick={() => {
            setAutoScroll(!autoScroll);
            userScrolledRef.current = false;
          }}
        />
        <ToolbarIconButton
          icon={<Download />}
          title="Export CSV"
          onClick={() => exportFramesToCsv(filtered)}
        />
        <ToolbarIconButton
          icon={<Trash2 />}
          variant="danger"
          title={t(lang, 'clear')}
          onClick={handleClear}
        />
      </div>

      {/* 表头 */}
      <div className="flex items-center text-[10px] uppercase text-text-secondary font-mono border-b border-border bg-bg-panel-header flex-shrink-0 select-none">
        <span className="px-3 py-1.5 text-left font-medium whitespace-nowrap min-w-[132px]">
          {t(lang, 'time')}
        </span>
        <span className="px-2 py-1.5 text-left font-medium whitespace-nowrap min-w-[56px]">Dir</span>
        <span className="px-3 py-1.5 text-left font-medium whitespace-nowrap min-w-[76px]">ID</span>
        <span className="px-2 py-1.5 text-center font-medium whitespace-nowrap min-w-[40px]">Ext</span>
        <span className="px-2 py-1.5 text-center font-medium whitespace-nowrap min-w-[40px]">RTR</span>
        <span className="px-2 py-1.5 text-center font-medium whitespace-nowrap min-w-[40px]">DLC</span>
        <span className="px-3 py-1.5 text-left font-medium whitespace-nowrap flex-1">Data</span>
      </div>

      {/* 虚拟滚动列表 */}
      <div
        className="flex-1 overflow-auto font-mono text-xs leading-relaxed min-h-0 outline-none"
        ref={parentRef}
        onScroll={handleScroll}
        onKeyDown={handleKeyDown}
        tabIndex={0}
      >
        {filtered.length === 0 ? (
          <div className="flex items-center justify-center h-48 text-text-secondary text-xs">
            {t(lang, 'noCanFrames')}
          </div>
        ) : (
          <div style={{ height: `${virtualizer.getTotalSize()}px`, position: 'relative' }}>
            <div
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: '100%',
                transform: `translateY(${virtualItems[0]?.start ?? 0}px)`,
              }}
            >
              {virtualItems.map((virtualRow) => {
                const frame = filtered[virtualRow.index];
                if (!frame) return null;
                return (
                  <Row
                    key={virtualRow.key}
                    index={virtualRow.index}
                    frame={frame}
                    isSelected={selection.isSelected(virtualRow.index)}
                    onMouseDown={handleRowMouseDown}
                  />
                );
              })}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
