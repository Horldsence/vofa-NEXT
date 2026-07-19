import { useState, useEffect, useRef, useMemo } from 'react';
import { useAppStore } from '../../store/appStore';
import { decodedEventBuffer } from '../../lib/logicBuffer';
import { clearDecodedBuffer } from '../../lib/logicSubscription';
import { t } from '../../i18n';
import { ToolbarIconButton } from '../ui/ToolbarIconButton';
import { Trash2, ArrowDown } from 'lucide-react';
import type { DecodedEvent, I2cEvent } from '../../types';

/// 格式化微秒时间戳为定宽字符串
function formatTs(us: number): string {
  return us.toString().padStart(10, '0');
}

/// 格式化 I2C 事件为字符串与颜色
function formatI2cEvent(event: I2cEvent): { text: string; color: string } {
  if ('Start' in event) return { text: 'START', color: '#89d185' };
  if ('Stop' in event) return { text: 'STOP', color: '#ff8c69' };
  if ('Address' in event) {
    const a = event.Address;
    return {
      text: `ADDR ${a.addr.toString(16).toUpperCase().padStart(2, '0')} ${a.read ? 'R' : 'W'} ${a.ack ? 'ACK' : 'NACK'}`,
      color: '#75beff',
    };
  }
  if ('Data' in event) {
    const d = event.Data;
    return {
      text: `DATA ${d.byte.toString(16).toUpperCase().padStart(2, '0')} ${d.ack ? 'ACK' : 'NACK'}`,
      color: '#dcdcaa',
    };
  }
  return { text: '?', color: '#888' };
}

/// 解码事件列表 — 支持 UART/I2C/SPI 类型过滤与自动滚动
export function DecodedEventList() {
  const lang = useAppStore((s) => s.lang);
  const [events, setEvents] = useState<DecodedEvent[]>([]);
  const [autoScroll, setAutoScroll] = useState(true);
  const [filterType, setFilterType] = useState<'all' | 'Uart' | 'I2c' | 'Spi'>('all');
  const listRef = useRef<HTMLDivElement>(null);
  const userScrolledRef = useRef(false);

  // 订阅 decodedEventBuffer
  useEffect(() => {
    const unsub = decodedEventBuffer.subscribe((recent) => {
      setEvents(recent);
    });
    setEvents(decodedEventBuffer.getRecent(500));
    return unsub;
  }, []);

  // 自动滚动到底部
  useEffect(() => {
    if (autoScroll && !userScrolledRef.current && listRef.current) {
      listRef.current.scrollTop = listRef.current.scrollHeight;
    }
  }, [events, autoScroll]);

  const handleScroll = () => {
    if (!listRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = listRef.current;
    userScrolledRef.current = scrollHeight - scrollTop - clientHeight > 30;
  };

  const handleClear = () => {
    void clearDecodedBuffer();
    decodedEventBuffer.clear();
    setEvents([]);
  };

  // 按协议类型过滤
  const filtered = useMemo(() => {
    if (filterType === 'all') return events;
    return events.filter((e) => filterType in e);
  }, [events, filterType]);

  const renderEvent = (e: DecodedEvent, i: number) => {
    if ('Uart' in e) {
      const u = e.Uart;
      return (
        <div key={i} className="grid grid-cols-[7rem_4rem_1fr] sm:grid-cols-[8rem_4rem_1fr] gap-2 px-3 sm:px-4 py-1 border-b border-border/30 hover:bg-bg-hover/60 transition-colors font-mono text-xs items-center">
          <span className="text-text-secondary">{formatTs(u.timestamp)}</span>
          <span className="text-green font-semibold">UART</span>
          <span className="flex items-center gap-2 min-w-0">
            <span className="text-text-bright">0x{u.byte.toString(16).toUpperCase().padStart(2, '0')}</span>
            <span className="text-text-secondary">({String.fromCharCode(u.byte)})</span>
            {!u.parity_ok && <span className="text-red text-[10px] border border-red/50 rounded px-1">PERR</span>}
          </span>
        </div>
      );
    }
    if ('I2c' in e) {
      const i2c = e.I2c;
      const { text, color } = formatI2cEvent(i2c.event);
      return (
        <div key={i} className="grid grid-cols-[7rem_4rem_1fr] sm:grid-cols-[8rem_4rem_1fr] gap-2 px-3 sm:px-4 py-1 border-b border-border/30 hover:bg-bg-hover/60 transition-colors font-mono text-xs items-center">
          <span className="text-text-secondary">{formatTs(i2c.timestamp)}</span>
          <span style={{ color }} className="font-semibold">I2C</span>
          <span style={{ color }} className="truncate">{text}</span>
        </div>
      );
    }
    if ('Spi' in e) {
      const s = e.Spi;
      return (
        <div key={i} className="grid grid-cols-[7rem_4rem_1fr] sm:grid-cols-[8rem_4rem_1fr] gap-2 px-3 sm:px-4 py-1 border-b border-border/30 hover:bg-bg-hover/60 transition-colors font-mono text-xs items-center">
          <span className="text-text-secondary">{formatTs(s.timestamp)}</span>
          <span className="text-orange font-semibold">SPI</span>
          <span className="flex items-center gap-2 min-w-0">
            <span className="text-text-bright">MOSI:0x{s.mosi.toString(16).toUpperCase().padStart(2, '0')}</span>
            <span className="text-text-primary">MISO:0x{s.miso.toString(16).toUpperCase().padStart(2, '0')}</span>
          </span>
        </div>
      );
    }
    return null;
  };

  return (
    <div className="h-full flex flex-col overflow-hidden bg-bg-editor">
      <div className="flex flex-wrap gap-2 p-2 items-center border-b border-border bg-bg-panel-header flex-shrink-0">
        <div className="flex items-center gap-0.5">
          {(['all', 'Uart', 'I2c', 'Spi'] as const).map((tp) => (
            <button
              key={tp}
              type="button"
              className={`px-2 py-0.5 text-[11px] border rounded cursor-pointer transition-all ${
                filterType === tp
                  ? 'bg-accent border-accent text-text-inverse'
                  : 'bg-bg-input text-text-secondary border-border hover:bg-bg-hover hover:text-text-primary'
              }`}
              onClick={() => setFilterType(tp)}
            >
              {tp === 'all' ? t(lang, 'all') : tp}
            </button>
          ))}
        </div>

        <div className="flex-1 min-w-2" />

        <span className="text-xs text-text-secondary font-mono">{events.length} events</span>
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
          icon={<Trash2 />}
          variant="danger"
          title={t(lang, 'clear')}
          onClick={handleClear}
        />
      </div>

      {/* 表头 */}
      <div className="grid grid-cols-[7rem_4rem_1fr] sm:grid-cols-[8rem_4rem_1fr] gap-2 px-3 sm:px-4 py-1 border-b border-border bg-bg-panel-header text-[10px] font-semibold uppercase tracking-wide text-text-secondary flex-shrink-0">
        <span>{t(lang, 'time')}</span>
        <span>Protocol</span>
        <span>Data</span>
      </div>

      <div
        className="flex-1 overflow-y-auto min-h-0"
        ref={listRef}
        onScroll={handleScroll}
      >
        {filtered.length === 0 ? (
          <div className="flex items-center justify-center h-48 text-text-secondary text-xs">
            {t(lang, 'noDecodedEvents')}
          </div>
        ) : (
          filtered.map(renderEvent)
        )}
      </div>
    </div>
  );
}
