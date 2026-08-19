import type { AppendMode } from './rawDataViewHelpers';
import { t, type Lang } from '../../../i18n';

interface Props {
  appendMode: AppendMode;
  sendContent: string;
  onAppendModeChange: (v: AppendMode) => void;
  onSendContentChange: (v: string) => void;
  onSend: () => void;
  lang: Lang;
  compact?: boolean;
  /// 当前发送目标标签 (只读展示; null = 无可用目标, 禁用发送)
  /// 目标选择已上移至头部: 全局模式用头部选择器, 单通道模式锁定为连线串口
  targetTransportLabel: string | null;
}

export function RawDataViewSendPanel({
  appendMode,
  sendContent,
  onAppendModeChange,
  onSendContentChange,
  onSend,
  lang,
  compact = false,
  targetTransportLabel,
}: Props) {
  const appendOptions: { mode: AppendMode; label: string }[] = [
    { mode: 'none', label: t(lang, 'appendNone') },
    { mode: 'nl', label: t(lang, 'appendNewline') },
    { mode: 'tab', label: t(lang, 'appendTab') },
    { mode: 'nl_tab', label: t(lang, 'appendNewlineTab') },
  ];

  const renderAppendOptions = (vertical = false) => (
    <div className={`flex ${vertical ? 'flex-col' : 'items-center'} gap-0.5 ${vertical ? '' : 'flex-shrink-0'}`}>
      {!vertical && <span className="text-xs text-text-secondary mr-0.5">{t(lang, 'appendSuffix')}:</span>}
      {appendOptions.map((opt) => (
        <button
          key={opt.mode}
          className={`px-1.5 py-0.5 bg-bg-input border border-border rounded-sm text-text-secondary text-xs font-mono cursor-pointer transition-all hover:border-accent hover:text-text-primary ${appendMode === opt.mode ? 'bg-accent border-accent text-text-inverse' : ''}`}
          onClick={() => onAppendModeChange(opt.mode)}
        >
          {opt.label}
        </button>
      ))}
    </div>
  );

  const renderSendInput = () => (
    <input
      type="text"
      className="flex-1 min-w-[60px] px-2 py-1 bg-bg-input text-text-primary border border-border rounded text-sm focus:outline-none focus:border-accent transition-colors"
      placeholder={lang === 'zh' ? '输入要发送的文本...' : 'Type to send...'}
      value={sendContent}
      onChange={(e) => onSendContentChange(e.target.value)}
      onKeyDown={(e) => {
        if (e.key === 'Enter') onSend();
      }}
    />
  );

  const renderSendButton = () => (
    <button
      className="px-3 py-1.5 bg-bg-button text-text-inverse border-none rounded cursor-pointer text-sm text-center transition-colors hover:bg-bg-button-hover disabled:opacity-50 disabled:cursor-default"
      onClick={onSend}
      disabled={!targetTransportLabel}
      title={!targetTransportLabel ? t(lang, 'noTransportNode') : undefined}
    >
      {t(lang, 'send')}
    </button>
  );

  const renderTargetLabel = () => (
    <span
      className="text-xs text-text-secondary whitespace-nowrap font-mono max-w-[160px] truncate"
      title={t(lang, 'targetTransport')}
    >
      → {targetTransportLabel ?? t(lang, 'noTransportNode')}
    </span>
  );

  if (compact) {
    return (
      <div className="flex flex-col gap-1.5">
        <span className="text-xs text-text-secondary">{t(lang, 'appendSuffix')}</span>
        {renderAppendOptions(true)}
        {renderTargetLabel()}
        {renderSendInput()}
        {renderSendButton()}
      </div>
    );
  }

  return (
    <div className="flex gap-1.5 p-1.5 items-center border-t border-border bg-bg-panel-header flex-shrink-0">
      {renderAppendOptions()}
      {renderTargetLabel()}
      {renderSendInput()}
      {renderSendButton()}
    </div>
  );
}
