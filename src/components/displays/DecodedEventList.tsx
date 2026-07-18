import { useState, useEffect, useRef, useMemo } from 'react';
import { useAppStore } from '../../store/appStore';
import { decodedEventBuffer } from '../../lib/logicBuffer';
import { clearDecodedBuffer } from '../../lib/logicSubscription';
import { t } from '../../i18n';
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
        <div key={i} className="flex gap-2 px-2 py-0.5 border-b border-border/30 hover:bg-bg-hover font-mono text-xs">
          <span className="text-text-secondary w-28">{formatTs(u.timestamp)}</span>
          <span className="text-green w-12">UART</span>
          <span className="text-text-bright">0x{u.byte.toString(16).toUpperCase().padStart(2, '0')}</span>
          <span className="text-text-secondary">({String.fromCharCode(u.byte)})</span>
          {!u.parity_ok && <span className="text-red">PERR</span>}
        </div>
      );
    }
    if ('I2c' in e) {
      const i2c = e.I2c;
      const { text, color } = formatI2cEvent(i2c.event);
      return (
        <div key={i} className="flex gap-2 px-2 py-0.5 border-b border-border/30 hover:bg-bg-hover font-mono text-xs">
          <span className="text-text-secondary w-28">{formatTs(i2c.timestamp)}</span>
          <span style={{ color }} className="w-12">I2C</span>
          <span style={{ color }}>{text}</span>
        </div>
      );
    }
    if ('Spi' in e) {
      const s = e.Spi;
      return (
        <div key={i} className="flex gap-2 px-2 py-0.5 border-b border-border/30 hover:bg-bg-hover font-mono text-xs">
          <span className="text-text-secondary w-28">{formatTs(s.timestamp)}</span>
          <span className="text-orange w-12">SPI</span>
          <span className="text-text-bright">MOSI:0x{s.mosi.toString(16).toUpperCase().padStart(2, '0')}</span>
          <span className="text-text-bright">MISO:0x{s.miso.toString(16).toUpperCase().padStart(2, '0')}</span>
        </div>
      );
    }
    return null;
  };

  return (
    <div className="h-full flex flex-col overflow-hidden">
      <div className="flex gap-1 p-1 items-center border-b border-border bg-bg-panel-header flex-shrink-0">
        <div className="flex items-center gap-0.5">
          {(['all', 'Uart', 'I2c', 'Spi'] as const).map((tp) => (
            <button
              key={tp}
              className={`px-1.5 py-0.5 text-xs border rounded cursor-pointer transition-all ${
                filterType === tp
                  ? 'bg-accent border-accent text-text-bright'
                  : 'bg-bg-input text-text-secondary border-border hover:bg-bg-hover'
              }`}
              onClick={() => setFilterType(tp)}
            >
              {tp === 'all' ? t(lang, 'all') : tp}
            </button>
          ))}
        </div>
        <div className="flex-1" />
        <span className="text-xs text-text-secondary font-mono">{events.length} events</span>
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
      <div
        className="flex-1 overflow-y-auto min-h-0"
        ref={listRef}
        onScroll={handleScroll}
      >
        {filtered.length === 0 ? (
          <div className="flex items-center justify-center h-full text-text-secondary text-xs">
            {t(lang, 'noDecodedEvents')}
          </div>
        ) : (
          filtered.map(renderEvent)
        )}
      </div>
    </div>
  );
}
