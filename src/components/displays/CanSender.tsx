import { useState } from 'react';
import { useAppStore } from '../../store/appStore';
import { sendCanFrame } from '../../lib/canSubscription';
import { t } from '../../i18n';
import { Send, Clock } from 'lucide-react';
import type { CanFrame, CanDirection } from '../../types';

/// 格式化数据为 HEX 字符串
function formatDataHex(data: number[]): string {
  return data.map((b) => b.toString(16).toUpperCase().padStart(2, '0')).join(' ');
}

/// CAN 帧发送器 — 表单输入 ID/扩展/远程/数据, 发送 + 历史记录
export function CanSender() {
  const lang = useAppStore((s) => s.lang);

  const [idText, setIdText] = useState('123');
  const [extended, setExtended] = useState(false);
  const [rtr, setRtr] = useState(false);
  const [dataText, setDataText] = useState('11 22 33');
  const [history, setHistory] = useState<CanFrame[]>([]);
  const [error, setError] = useState('');

  const parseData = (text: string): number[] => {
    const cleaned = text.trim().replace(/\s+/g, ' ');
    if (!cleaned) return [];
    return cleaned.split(' ').map((h) => {
      const b = parseInt(h, 16);
      if (isNaN(b) || b < 0 || b > 255) throw new Error(`无效字节: ${h}`);
      return b;
    });
  };

  const handleSend = async () => {
    setError('');
    try {
      const id = parseInt(idText.replace(/^0x/i, ''), 16);
      if (isNaN(id)) throw new Error('无效 ID');
      if (extended && id > 0x1FFFFFFF) throw new Error('扩展 ID 超出 29 位');
      if (!extended && id > 0x7FF) throw new Error('标准 ID 超出 11 位');

      const data = rtr ? [] : parseData(dataText);
      if (data.length > 8) throw new Error('数据长度超过 8 字节');

      const frame: CanFrame = {
        timestamp: Date.now() * 1000,
        id,
        extended,
        rtr,
        dlc: data.length,
        data,
        direction: 'Tx' as CanDirection,
      };

      await sendCanFrame(frame);
      setHistory((prev) => [...prev.slice(-9), frame]);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div className="h-full flex flex-col overflow-hidden">
      <div className="p-3 border-b border-border bg-bg-panel-header flex-shrink-0">
        <h3 className="text-sm font-semibold text-text-primary mb-2">{t(lang, 'canSender')}</h3>
        <div className="grid grid-cols-2 gap-2 mb-2">
          <div>
            <label className="block text-[10px] text-text-secondary mb-0.5">CAN ID (HEX)</label>
            <input
              type="text"
              className="w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded text-sm font-mono focus:outline-none focus:border-accent"
              value={idText}
              onChange={(e) => setIdText(e.target.value)}
              placeholder="123"
            />
          </div>
          <div>
            <label className="block text-[10px] text-text-secondary mb-0.5">DLC</label>
            <input
              type="number"
              min={0}
              max={8}
              className="w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded text-sm focus:outline-none focus:border-accent"
              value={rtr ? 0 : (parseData(dataText).length || 0)}
              readOnly
            />
          </div>
        </div>
        <div className="flex gap-3 mb-2 text-xs">
          <label className="flex items-center gap-1 cursor-pointer">
            <input
              type="checkbox"
              checked={extended}
              onChange={(e) => setExtended(e.target.checked)}
              className="accent-accent"
            />
            <span>{t(lang, 'extendedFrame')}</span>
          </label>
          <label className="flex items-center gap-1 cursor-pointer">
            <input
              type="checkbox"
              checked={rtr}
              onChange={(e) => setRtr(e.target.checked)}
              className="accent-accent"
            />
            <span>{t(lang, 'remoteFrame')}</span>
          </label>
        </div>
        {!rtr && (
          <div className="mb-2">
            <label className="block text-[10px] text-text-secondary mb-0.5">{t(lang, 'dataHex')}</label>
            <input
              type="text"
              className="w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded text-sm font-mono focus:outline-none focus:border-accent"
              value={dataText}
              onChange={(e) => setDataText(e.target.value)}
              placeholder="11 22 33 44 55 66 77 88"
            />
          </div>
        )}
        {error && (
          <div className="mb-2 px-2 py-1 bg-red-900/30 border border-red text-red text-xs rounded">
            {error}
          </div>
        )}
        <button
          className="w-full px-3 py-1.5 bg-blue text-white border-none rounded cursor-pointer text-sm flex items-center justify-center gap-1.5 hover:bg-blue/80 transition-colors"
          onClick={handleSend}
        >
          <Send size={14} />
          {t(lang, 'send')}
        </button>
      </div>

      {/* 发送历史 */}
      <div className="flex-1 overflow-y-auto min-h-0">
        <div className="px-3 py-1.5 border-b border-border text-[10px] font-semibold uppercase text-text-secondary flex items-center gap-1">
          <Clock size={11} />
          {t(lang, 'sendHistory')}
        </div>
        {history.length === 0 ? (
          <div className="flex items-center justify-center h-32 text-text-secondary text-xs">
            {t(lang, 'noSendHistory')}
          </div>
        ) : (
          <div className="font-mono text-xs">
            {history.slice().reverse().map((f, i) => (
              <div key={i} className="px-3 py-1 border-b border-border/30 hover:bg-bg-hover flex gap-2 items-center">
                <span className="text-blue">→ Tx</span>
                <span className="text-text-bright">
                  {f.extended ? f.id.toString(16).toUpperCase().padStart(8, '0') : f.id.toString(16).toUpperCase().padStart(3, '0')}
                </span>
                {f.extended && <span className="text-text-secondary text-[10px]">EXT</span>}
                {f.rtr && <span className="text-text-secondary text-[10px]">RTR</span>}
                <span className="text-text-secondary">[{f.dlc}]</span>
                <span className="text-text-primary">{f.rtr ? '(remote)' : formatDataHex(f.data)}</span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
