import { Send, AlertTriangle } from 'lucide-react';
import type { WidgetConfig } from '../../../types';
import type { Lang } from '../../../i18n';
import { t } from '../../../i18n';
import { bytesToHex, bytesToAscii } from '../../../lib/utils/commandParser';

interface Props {
  params: Extract<WidgetConfig, { kind: 'Command' }>['params'];
  computed: {
    bytes: Uint8Array | null;
    error: string | null;
    perBlock: Uint8Array[][];
  };
  error: string | null;
  lastSent: string | null;
  /// 无字节边路由 (loopbackOut 未连接) — 禁用发送并提示
  routeMissing: boolean;
  onSend: () => void;
  onUpdateParams: (changes: Partial<Props['params']>) => void;
  lang: Lang;
}

export function CommandSenderSidebar({
  params,
  computed,
  error,
  lastSent,
  routeMissing,
  onSend,
  onUpdateParams,
  lang,
}: Props) {
  const sendMode = params.sendMode ?? 'manual';
  const timerMs = params.timerMs ?? 100;

  return (
    <div className="w-[300px] flex-shrink-0 border-l border-border bg-bg-sidebar overflow-y-auto flex flex-col gap-2 p-3">
      {/* 预览 */}
      <div className="text-[10px] text-text-secondary uppercase tracking-wide font-semibold">{t(lang, 'cmdPreview')}</div>
      <div className="bg-bg-editor border border-border rounded px-2 py-1.5 flex flex-col gap-1">
        <div className="flex items-center justify-between text-[10px] text-text-secondary uppercase tracking-wide">
          <span>HEX</span>
          {computed.bytes && (
            <span className="font-mono text-blue">{computed.bytes.length}B</span>
          )}
        </div>
        {computed.error ? (
          <div className="flex items-center gap-1 bg-red/10 border border-red/30 text-red px-1.5 py-1 rounded-sm text-xs">
            <AlertTriangle size={11} />
            <span>{computed.error}</span>
          </div>
        ) : computed.bytes && computed.bytes.length > 0 ? (
          <>
            <div className="font-mono text-sm text-green break-all leading-relaxed">
              {bytesToHex(computed.bytes)}
            </div>
            <div className="font-mono text-xs text-text-secondary break-all leading-relaxed opacity-85">
              {bytesToAscii(computed.bytes)}
            </div>
          </>
        ) : (
          <div className="text-xs text-text-secondary opacity-60 italic py-1">{t(lang, 'cmdPreviewEmpty')}</div>
        )}
      </div>

      {/* 发送 */}
      <button
        className="justify-center px-4 py-1.5 bg-bg-button text-text-inverse border-none rounded cursor-pointer text-sm transition-colors hover:bg-bg-button-hover font-semibold inline-flex items-center gap-1.5 disabled:opacity-50 disabled:cursor-default"
        onClick={onSend}
        disabled={routeMissing || !computed.bytes || computed.bytes.length === 0 || !!computed.error}
        title={routeMissing ? t(lang, 'cmdNoByteRoute') : undefined}
      >
        <Send size={12} />
        <span>{t(lang, 'cmdSend')}</span>
      </button>

      {routeMissing && (
        <div className="flex items-center gap-1 bg-yellow/10 border border-yellow/30 text-yellow px-1.5 py-1 rounded-sm text-xs">
          <AlertTriangle size={11} />
          <span>{t(lang, 'cmdNoByteRoute')}</span>
        </div>
      )}

      {error && (
        <div className="flex items-center gap-1 bg-red/10 border border-red/30 text-red px-1.5 py-1 rounded-sm text-xs">
          <AlertTriangle size={11} />
          <span>{error}</span>
        </div>
      )}
      {lastSent && (
        <div className="flex items-center gap-1 px-1.5 py-1 bg-bg-editor rounded-sm text-[10px]" title={lastSent}>
          <span className="text-text-secondary flex-shrink-0">{t(lang, 'cmdLastSent')}:</span>
          <span className="font-mono text-text-primary whitespace-nowrap overflow-hidden text-ellipsis">{lastSent}</span>
        </div>
      )}

      {/* 全局设置 */}
      <div className="text-[10px] text-text-secondary uppercase tracking-wide font-semibold pt-1">{t(lang, 'cmdSettings')}</div>
      <div className="flex flex-col gap-2 p-2 bg-bg-editor border border-border rounded">
        <div className="grid grid-cols-[80px_1fr] items-center gap-2">
          <label className="text-xs text-text-secondary">{t(lang, 'cmdLabel')}</label>
          <input
            type="text"
            value={params.label}
            onChange={(e) => onUpdateParams({ label: e.target.value })}
            className="text-xs w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded focus:outline-none focus:border-accent transition-colors"
          />
        </div>
        <div className="grid grid-cols-[80px_1fr] items-center gap-2">
          <label className="text-xs text-text-secondary">{t(lang, 'cmdAppendNewline')}</label>
          <button
            className={`bg-bg-input border border-border text-text-secondary px-2 py-0.5 text-xs rounded-sm cursor-pointer transition-all hover:text-text-primary ${params.appendNewline ? 'bg-bg-button text-text-inverse border-bg-button' : ''}`}
            onClick={() => onUpdateParams({ appendNewline: !params.appendNewline })}
          >
            {params.appendNewline ? t(lang, 'cmdNewlineOn') : t(lang, 'cmdNewlineOff')}
          </button>
        </div>
        <div className="grid grid-cols-[80px_1fr] items-center gap-2">
          <label className="text-xs text-text-secondary">{t(lang, 'cmdSendMode')}</label>
          <select
            value={sendMode}
            onChange={(e) => onUpdateParams({ sendMode: e.target.value as 'manual' | 'onChange' | 'timer' })}
            className="text-xs w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded focus:outline-none focus:border-accent"
          >
            <option value="manual">{t(lang, 'cmdSendModeManual')}</option>
            <option value="onChange">{t(lang, 'cmdSendModeOnChange')}</option>
            <option value="timer">{t(lang, 'cmdSendModeTimer')}</option>
          </select>
        </div>
        {sendMode === 'timer' && (
          <div className="grid grid-cols-[80px_1fr] items-center gap-2">
            <label className="text-xs text-text-secondary">{t(lang, 'cmdSendModeInterval')}</label>
            <input
              type="number"
              min={10}
              max={10000}
              value={timerMs}
              onChange={(e) => onUpdateParams({ timerMs: parseInt(e.target.value) || 100 })}
              className="text-xs w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded focus:outline-none focus:border-accent"
            />
          </div>
        )}
      </div>

      {/* 回环模式设置 */}
      <div className="text-[10px] text-text-secondary uppercase tracking-wide font-semibold pt-2">{t(lang, 'cmdLoopback')}</div>
      <div className="flex flex-col gap-2 p-2 bg-bg-editor border border-border rounded">
        <div className="grid grid-cols-[80px_1fr] items-center gap-2">
          <label className="text-xs text-text-secondary">{t(lang, 'cmdLoopback')}</label>
          <button
            className={`bg-bg-input border border-border text-text-secondary px-2 py-0.5 text-xs rounded-sm cursor-pointer transition-all hover:text-text-primary ${params.loopbackEnabled ? 'bg-bg-button text-text-inverse border-bg-button' : ''}`}
            onClick={() => onUpdateParams({ loopbackEnabled: !params.loopbackEnabled })}
          >
            {params.loopbackEnabled ? t(lang, 'cmdNewlineOn') : t(lang, 'cmdNewlineOff')}
          </button>
        </div>
      </div>
    </div>
  );
}
