import { useState } from 'react';
import { useAppStore } from '../../store/appStore';
import { sendCanFrame } from '../../lib/canSubscription';
import { t } from '../../i18n';
import { Send, Clock, AlertCircle } from 'lucide-react';
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
    <div className="h-full flex flex-col overflow-hidden bg-bg-editor">
      {/* 发送表单 */}
      <div className="p-3 sm:p-4 border-b border-border bg-bg-panel-header flex-shrink-0 overflow-y-auto">
        <h3 className="text-sm font-semibold text-text-primary mb-3">{t(lang, 'canSender')}</h3>

        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 mb-3">
          <div>
            <label className="block text-[10px] uppercase tracking-wide text-text-secondary mb-1">CAN ID (HEX)</label>
            <input
              type="text"
              className="w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded text-sm font-mono focus:outline-none focus:border-accent transition-colors"
              value={idText}
              onChange={(e) => setIdText(e.target.value)}
              placeholder="123"
            />
          </div>
          <div>
            <label className="block text-[10px] uppercase tracking-wide text-text-secondary mb-1">DLC</label>
            <input
              type="number"
              min={0}
              max={8}
              className="w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded text-sm focus:outline-none focus:border-accent transition-colors"
              value={rtr ? 0 : (parseData(dataText).length || 0)}
              readOnly
            />
          </div>
        </div>

        <div className="flex flex-wrap gap-4 mb-3 text-xs">
          <label className="flex items-center gap-2 cursor-pointer select-none">
            <input
              type="checkbox"
              checked={extended}
              onChange={(e) => setExtended(e.target.checked)}
              className="accent-accent w-3.5 h-3.5"
            />
            <span className="text-text-secondary">{t(lang, 'extendedFrame')}</span>
          </label>
          <label className="flex items-center gap-2 cursor-pointer select-none">
            <input
              type="checkbox"
              checked={rtr}
              onChange={(e) => setRtr(e.target.checked)}
              className="accent-accent w-3.5 h-3.5"
            />
            <span className="text-text-secondary">{t(lang, 'remoteFrame')}</span>
          </label>
        </div>

        {!rtr && (
          <div className="mb-3">
            <label className="block text-[10px] uppercase tracking-wide text-text-secondary mb-1">{t(lang, 'dataHex')}</label>
            <input
              type="text"
              className="w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded text-sm font-mono focus:outline-none focus:border-accent transition-colors"
              value={dataText}
              onChange={(e) => setDataText(e.target.value)}
              placeholder="11 22 33 44 55 66 77 88"
            />
          </div>
        )}

        {error && (
          <div className="mb-3 px-2.5 py-1.5 bg-bg-danger/30 border border-red/50 rounded flex items-start gap-2 text-xs text-red">
            <AlertCircle size={14} className="flex-shrink-0 mt-0.5" />
            <span>{error}</span>
          </div>
        )}

        <button
          type="button"
          className="w-full px-3 py-1.5 bg-bg-button text-text-inverse border-none rounded cursor-pointer text-sm flex items-center justify-center gap-1.5 hover:bg-bg-button-hover transition-colors"
          onClick={handleSend}
        >
          <Send size={14} />
          {t(lang, 'send')}
        </button>
      </div>

      {/* 发送历史 */}
      <div className="flex-1 overflow-y-auto min-h-0">
        <div className="px-3 sm:px-4 py-1.5 border-b border-border text-[10px] font-semibold uppercase tracking-wide text-text-secondary flex items-center gap-1.5 sticky top-0 bg-bg-panel-header z-10">
          <Clock size={11} />
          {t(lang, 'sendHistory')}
          <span className="ml-auto text-text-secondary font-mono">{history.length}</span>
        </div>
        {history.length === 0 ? (
          <div className="flex items-center justify-center h-32 text-text-secondary text-xs">
            {t(lang, 'noSendHistory')}
          </div>
        ) : (
          <div className="font-mono text-xs">
            {history.slice().reverse().map((f, i) => (
              <div
                key={i}
                className="px-3 sm:px-4 py-1.5 border-b border-border/30 hover:bg-bg-hover/60 transition-colors flex flex-wrap gap-x-3 gap-y-0.5 items-center"
              >
                <span className="text-purple font-semibold">→ Tx</span>
                <span className="text-text-bright">
                  0x{f.extended ? f.id.toString(16).toUpperCase().padStart(8, '0') : f.id.toString(16).toUpperCase().padStart(3, '0')}
                </span>
                {f.extended && <span className="text-text-secondary text-[10px] border border-border rounded px-1">EXT</span>}
                {f.rtr && <span className="text-text-secondary text-[10px] border border-border rounded px-1">RTR</span>}
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
