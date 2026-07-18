import { useState, useEffect, useRef, useMemo } from 'react';
import { useAppStore } from '../../store/appStore';
import { canFrameBuffer } from '../../lib/canBuffer';
import { clearCanBuffer } from '../../lib/canSubscription';
import { t } from '../../i18n';
import { ToolbarIconButton } from '../ui/ToolbarIconButton';
import { Trash2, ArrowDown, Filter, Download } from 'lucide-react';
import type { CanFrame } from '../../types';

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

/// 将帧列表导出为 CSV
function exportFramesToCsv(frames: CanFrame[]): void {
  const rows = [
    ['Time', 'Dir', 'ID', 'Extended', 'RTR', 'DLC', 'Data'].join(','),
    ...frames.map((f) => [
      formatCanTime(f.timestamp),
      f.direction,
      formatCanId(f.id, f.extended),
      f.extended ? 'X' : '',
      f.rtr ? 'R' : '',
      f.dlc,
      f.rtr ? '' : formatDataHex(f.data),
    ].join(',')),
  ];
  const blob = new Blob([rows.join('\n')], { type: 'text/csv;charset=utf-8;' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = `can-frames-${new Date().toISOString().replace(/[:.]/g, '-')}.csv`;
  a.click();
  URL.revokeObjectURL(url);
}

/// CAN 帧列表显示 — 表格视图, 支持按 ID/方向过滤, 自动滚动
export function CanFrameList() {
  const lang = useAppStore((s) => s.lang);

  const [frames, setFrames] = useState<CanFrame[]>([]);
  const [autoScroll, setAutoScroll] = useState(true);
  const [filterId, setFilterId] = useState('');
  const [filterDir, setFilterDir] = useState<'all' | 'Rx' | 'Tx'>('all');
  const listRef = useRef<HTMLDivElement>(null);
  const userScrolledRef = useRef(false);

  // 订阅 canFrameBuffer (RAF 节流后触发, 单一数据源)
  useEffect(() => {
    const unsub = canFrameBuffer.subscribe((recent) => setFrames(recent));
    setFrames(canFrameBuffer.getRecent(500));
    return unsub;
  }, []);

  // 自动滚动
  useEffect(() => {
    if (autoScroll && !userScrolledRef.current && listRef.current) {
      listRef.current.scrollTop = listRef.current.scrollHeight;
    }
  }, [frames, autoScroll]);

  const handleScroll = () => {
    if (!listRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = listRef.current;
    const atBottom = scrollHeight - scrollTop - clientHeight < 30;
    userScrolledRef.current = !atBottom;
  };

  const handleClear = () => {
    void clearCanBuffer();
    canFrameBuffer.clear();
    setFrames([]);
    userScrolledRef.current = false;
  };

  // 过滤
  const filtered = useMemo(() => {
    let result = frames;
    if (filterDir !== 'all') {
      result = result.filter((f) => f.direction === filterDir);
    }
    if (filterId.trim()) {
      const idLower = filterId.trim().toLowerCase().replace(/^0x/, '');
      const idNum = parseInt(idLower, 16);
      if (!isNaN(idNum)) {
        result = result.filter((f) => f.id === idNum);
      }
    }
    return result;
  }, [frames, filterId, filterDir]);

  const rxCount = frames.filter((f) => f.direction === 'Rx').length;
  const txCount = frames.filter((f) => f.direction === 'Tx').length;

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
                  ? 'bg-accent border-accent text-text-bright'
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
          <span className="hidden sm:inline">{frames.length}</span>
          <span className="px-1.5 py-0.5 rounded bg-green/10 text-green">Rx:{rxCount}</span>
          <span className="px-1.5 py-0.5 rounded bg-purple/10 text-purple">Tx:{txCount}</span>
        </div>

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

      {/* 帧列表 */}
      <div
        className="flex-1 overflow-auto font-mono text-xs leading-relaxed min-h-0"
        ref={listRef}
        onScroll={handleScroll}
      >
        <table className="w-full min-w-[640px]">
          <thead className="sticky top-0 bg-bg-panel-header border-b border-border z-10">
            <tr className="text-text-secondary text-[10px] uppercase">
              <th className="px-3 py-1.5 text-left font-medium whitespace-nowrap">{t(lang, 'time')}</th>
              <th className="px-2 py-1.5 text-left font-medium whitespace-nowrap">Dir</th>
              <th className="px-3 py-1.5 text-left font-medium whitespace-nowrap">ID</th>
              <th className="px-2 py-1.5 text-center font-medium whitespace-nowrap">Ext</th>
              <th className="px-2 py-1.5 text-center font-medium whitespace-nowrap">RTR</th>
              <th className="px-2 py-1.5 text-center font-medium whitespace-nowrap">DLC</th>
              <th className="px-3 py-1.5 text-left font-medium whitespace-nowrap">Data</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map((f, i) => (
              <tr
                key={i}
                className={`border-b border-border/30 hover:bg-bg-hover/60 transition-colors ${f.direction === 'Tx' ? 'text-text-primary' : 'text-text-primary'}`}
              >
                <td className="px-3 py-1 text-accent whitespace-nowrap">{formatCanTime(f.timestamp)}</td>
                <td className="px-2 py-1 whitespace-nowrap">
                  <span className={`inline-flex items-center gap-1 text-[10px] font-semibold ${f.direction === 'Tx' ? 'text-purple' : 'text-green'}`}>
                    {f.direction === 'Tx' ? '→ Tx' : '← Rx'}
                  </span>
                </td>
                <td className="px-3 py-1 text-text-bright whitespace-nowrap">0x{formatCanId(f.id, f.extended)}</td>
                <td className="px-2 py-1 text-center text-text-secondary">{f.extended ? 'X' : ''}</td>
                <td className="px-2 py-1 text-center text-text-secondary">{f.rtr ? 'R' : ''}</td>
                <td className="px-2 py-1 text-center text-text-secondary">{f.dlc}</td>
                <td className="px-3 py-1 text-text-primary">
                  {f.rtr ? <span className="text-text-secondary italic">(remote)</span> : formatDataHex(f.data)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {filtered.length === 0 && (
          <div className="flex items-center justify-center h-48 text-text-secondary text-xs">
            {t(lang, 'noCanFrames')}
          </div>
        )}
      </div>
    </div>
  );
}
