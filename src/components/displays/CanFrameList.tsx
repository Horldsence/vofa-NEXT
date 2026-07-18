import { useState, useEffect, useRef, useMemo } from 'react';
import { useAppStore } from '../../store/appStore';
import { canFrameBuffer } from '../../lib/canBuffer';
import { clearCanBuffer } from '../../lib/canSubscription';
import { t } from '../../i18n';
import { Trash2, ArrowDown, Filter } from 'lucide-react';
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

/// CAN 帧列表显示 — 表格视图, 支持按 ID/方向过滤, 自动滚动
export function CanFrameList() {
  const lang = useAppStore((s) => s.lang);
  const canFramesVersion = useAppStore((s) => s.canFramesVersion);

  const [frames, setFrames] = useState<CanFrame[]>([]);
  const [autoScroll, setAutoScroll] = useState(true);
  const [filterId, setFilterId] = useState('');
  const [filterDir, setFilterDir] = useState<'all' | 'Rx' | 'Tx'>('all');
  const listRef = useRef<HTMLDivElement>(null);
  const userScrolledRef = useRef(false);

  // 订阅 canFrameBuffer
  useEffect(() => {
    const unsub = canFrameBuffer.subscribe((recent) => {
      setFrames(recent);
    });
    setFrames(canFrameBuffer.getRecent(500));
    return unsub;
  }, []);

  // 自动滚动
  useEffect(() => {
    if (autoScroll && !userScrolledRef.current && listRef.current) {
      listRef.current.scrollTop = listRef.current.scrollHeight;
    }
  }, [frames, autoScroll, canFramesVersion]);

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
    <div className="h-full flex flex-col overflow-hidden">
      {/* 工具栏 */}
      <div className="flex gap-1 p-1 items-center border-b border-border bg-bg-panel-header flex-shrink-0">
        <div className="flex items-center gap-1">
          <Filter size={12} className="text-text-secondary" />
          <input
            type="text"
            className="w-24 px-1.5 py-0.5 bg-bg-input text-text-primary border border-border rounded text-xs focus:outline-none focus:border-accent"
            placeholder="ID (HEX)"
            value={filterId}
            onChange={(e) => setFilterId(e.target.value)}
          />
        </div>
        <div className="flex items-center gap-0.5">
          {(['all', 'Rx', 'Tx'] as const).map((d) => (
            <button
              key={d}
              className={`px-1.5 py-0.5 text-xs border rounded cursor-pointer transition-all ${
                filterDir === d
                  ? 'bg-accent border-accent text-text-bright'
                  : 'bg-bg-input text-text-secondary border-border hover:bg-bg-hover'
              }`}
              onClick={() => setFilterDir(d)}
            >
              {d === 'all' ? t(lang, 'all') : d}
            </button>
          ))}
        </div>
        <div className="flex-1" />
        <span className="text-xs text-text-secondary font-mono">
          {frames.length} (Rx:{rxCount} Tx:{txCount})
        </span>
        <button
          className={`w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer ${autoScroll && !userScrolledRef.current ? 'text-text-bright' : ''}`}
          title={t(lang, 'autoScroll')}
          onClick={() => {
            setAutoScroll(!autoScroll);
            userScrolledRef.current = false;
          }}
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

      {/* 帧列表 */}
      <div
        className="flex-1 overflow-y-auto font-mono text-xs leading-relaxed min-h-0"
        ref={listRef}
        onScroll={handleScroll}
      >
        <table className="w-full">
          <thead className="sticky top-0 bg-bg-panel-header border-b border-border">
            <tr className="text-text-secondary text-[10px] uppercase">
              <th className="px-2 py-1 text-left font-medium">{t(lang, 'time')}</th>
              <th className="px-1 py-1 text-left font-medium">Dir</th>
              <th className="px-2 py-1 text-left font-medium">ID</th>
              <th className="px-1 py-1 text-left font-medium">Ext</th>
              <th className="px-1 py-1 text-left font-medium">RTR</th>
              <th className="px-1 py-1 text-left font-medium">DLC</th>
              <th className="px-2 py-1 text-left font-medium">Data</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map((f, i) => (
              <tr
                key={i}
                className={`border-b border-border/30 hover:bg-bg-hover ${f.direction === 'Tx' ? 'text-blue' : 'text-text-primary'}`}
              >
                <td className="px-2 py-0.5 text-accent whitespace-nowrap">{formatCanTime(f.timestamp)}</td>
                <td className="px-1 py-0.5">
                  <span className={f.direction === 'Tx' ? 'text-blue' : 'text-green'}>
                    {f.direction === 'Tx' ? '→ Tx' : '← Rx'}
                  </span>
                </td>
                <td className="px-2 py-0.5 text-text-bright whitespace-nowrap">{formatCanId(f.id, f.extended)}</td>
                <td className="px-1 py-0.5 text-text-secondary">{f.extended ? 'X' : ''}</td>
                <td className="px-1 py-0.5 text-text-secondary">{f.rtr ? 'R' : ''}</td>
                <td className="px-1 py-0.5 text-text-secondary">{f.dlc}</td>
                <td className="px-2 py-0.5 text-text-primary">
                  {f.rtr ? '(remote)' : formatDataHex(f.data)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {filtered.length === 0 && (
          <div className="flex items-center justify-center h-full text-text-secondary text-xs">
            {t(lang, 'noCanFrames')}
          </div>
        )}
      </div>
    </div>
  );
}
