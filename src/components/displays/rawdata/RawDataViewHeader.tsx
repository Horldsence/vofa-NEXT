import {
  Trash2,
  ArrowDown,
  Clock,
  Settings2,
  Search,
  X,
  Copy,
  Check,
  FileWarning,
} from 'lucide-react';
import { t } from '../../../i18n';
import type { Lang } from '../../../i18n';
import type { RawDataGrouping, RawDataRepr, DirectionFilter } from './rawDataViewHelpers';
import { directionColorClass } from './rawDataViewHelpers';

const GROUPING_OPTIONS: { value: RawDataGrouping; label: string }[] = [
  { value: 'grid', label: 'gridView' },
  { value: 'line', label: 'lineView' },
];

const REPR_OPTIONS: { value: RawDataRepr; label: string }[] = [
  { value: 'hex', label: 'hexView' },
  { value: 'ascii', label: 'asciiView' },
];

const DIRECTION_OPTIONS: { value: DirectionFilter; label: string; symbol: string }[] = [
  { value: 'all', label: 'rawDataDirectionAll', symbol: '↕' },
  { value: 'rx', label: 'rawDataDirectionRx', symbol: '↓' },
  { value: 'tx', label: 'rawDataDirectionTx', symbol: '↑' },
];

interface ChannelOption {
  key: string;
  sourceId: string;
  sourceHandle: string | undefined;
  /// 字节平面源 (Transport/Protocol) 的完整标签 (如 "Serial (a1b2)·rx") — 缺省回退 handle/widget 标签
  label?: string;
}

interface Props {
  // state
  grouping: RawDataGrouping;
  repr: RawDataRepr;
  directionFilter: DirectionFilter;
  searchTerm: string;
  channel: string;
  autoScroll: boolean;
  showTimestamp: boolean;
  showSettings: boolean;
  isNum: boolean;
  isFiltered: boolean;
  totalBytes: number;
  modeCount: number;
  droppedBytes: number;
  channelOptions: ChannelOption[];
  /// 串口选择 (字节源 = 发送目标) — 仅全局模式渲染
  transportOptions: { id: string; label: string }[];
  selectedTransport: string | null;
  onTransportChange: (id: string) => void;
  selectionCount: number;
  copyFeedback: boolean;
  userScrolledRef: React.MutableRefObject<boolean>;
  lang: Lang;
  sourceLabel: (id: string) => string;
  // setters
  onGroupingChange: (v: RawDataGrouping) => void;
  onReprChange: (v: RawDataRepr) => void;
  onDirectionFilterChange: (v: DirectionFilter) => void;
  onSearchTermChange: (v: string) => void;
  onChannelChange: (v: string) => void;
  onAutoScrollChange: (v: boolean) => void;
  onShowTimestampChange: (v: boolean) => void;
  onShowSettingsChange: (v: boolean) => void;
  onClear: () => void;
  onClearSelection: () => void;
  onCopySelected: () => void;
  onDroppedInfoOpen: () => void;
}

export function RawDataViewHeader({
  grouping,
  repr,
  directionFilter,
  searchTerm,
  channel,
  autoScroll,
  showTimestamp,
  showSettings,
  isNum,
  isFiltered,
  totalBytes,
  modeCount,
  droppedBytes,
  channelOptions,
  transportOptions,
  selectedTransport,
  onTransportChange,
  selectionCount,
  copyFeedback,
  userScrolledRef,
  lang,
  sourceLabel,
  onGroupingChange,
  onReprChange,
  onDirectionFilterChange,
  onSearchTermChange,
  onChannelChange,
  onAutoScrollChange,
  onShowTimestampChange,
  onShowSettingsChange,
  onClear,
  onClearSelection,
  onCopySelected,
  onDroppedInfoOpen,
}: Props) {
  return (
    <>
      <div className="flex gap-1 p-1.5 items-center border-b border-border bg-bg-panel-header shrink-0">
        <div className="flex items-center bg-bg-input rounded p-0.5">
          {GROUPING_OPTIONS.map((opt) => (
            <button
              key={opt.value}
              disabled={isNum}
              className={`px-2 py-0.5 rounded-sm text-xs font-medium transition-all duration-150 motion-safe:active:scale-95 cursor-pointer disabled:cursor-not-allowed disabled:pointer-events-none disabled:opacity-40 ${grouping === opt.value ? 'bg-bg-button text-text-inverse' : 'text-text-secondary hover:text-text-primary'}`}
              onClick={() => onGroupingChange(opt.value)}
            >
              {t(lang, opt.label)}
            </button>
          ))}
        </div>
        <div className="flex items-center bg-bg-input rounded p-0.5">
          {REPR_OPTIONS.map((opt) => (
            <button
              key={opt.value}
              disabled={isNum}
              className={`px-2 py-0.5 rounded-sm text-xs font-medium transition-all duration-150 motion-safe:active:scale-95 cursor-pointer disabled:cursor-not-allowed disabled:pointer-events-none disabled:opacity-40 ${repr === opt.value ? 'bg-bg-button text-text-inverse' : 'text-text-secondary hover:text-text-primary'}`}
              onClick={() => onReprChange(opt.value)}
            >
              {t(lang, opt.label)}
            </button>
          ))}
        </div>

        <div className="flex items-center bg-bg-input rounded p-0.5">
          {DIRECTION_OPTIONS.map((opt) => (
            <button
              key={opt.value}
              disabled={isNum}
              title={t(lang, opt.label)}
              className={`px-2 py-0.5 rounded-sm text-xs font-medium transition-all duration-150 motion-safe:active:scale-95 cursor-pointer disabled:cursor-not-allowed disabled:pointer-events-none disabled:opacity-40 ${directionFilter === opt.value ? 'bg-bg-button text-text-inverse' : 'text-text-secondary hover:text-text-primary'}`}
              onClick={() => onDirectionFilterChange(opt.value)}
            >
              <span className={`mr-1 ${directionFilter === opt.value ? 'text-text-inverse' : opt.value === 'all' ? 'text-text-secondary' : directionColorClass(opt.value)}`}>
                {opt.symbol}
              </span>
              {t(lang, opt.label)}
            </button>
          ))}
        </div>

        <div className={`flex items-center gap-1 bg-bg-input rounded px-1.5 py-0.5 border border-border ${isNum ? 'opacity-40 pointer-events-none' : ''}`}>
          <Search size={12} className="text-text-secondary shrink-0" />
          <input
            type="text"
            disabled={isNum}
            className="bg-transparent border-none outline-none text-xs text-text-primary placeholder:text-text-disabled min-w-[80px] w-[120px]"
            placeholder={t(lang, 'rawDataSearchPlaceholder')}
            value={searchTerm}
            onChange={(e) => onSearchTermChange(e.target.value)}
          />
          {searchTerm && (
            <button
              className="text-text-secondary hover:text-text-primary"
              onClick={() => onSearchTermChange('')}
            >
              <X size={12} />
            </button>
          )}
        </div>

        {channel === 'global' && transportOptions.length > 0 && (
          <select
            className="bg-bg-input border border-border rounded px-1 py-0.5 text-xs font-mono text-text-primary transition-colors hover:border-accent focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent/40 cursor-pointer max-w-[140px]"
            value={selectedTransport ?? ''}
            onChange={(e) => onTransportChange(e.target.value)}
            title={t(lang, 'targetTransport')}
          >
            {transportOptions.map((tr) => (
              <option key={tr.id} value={tr.id}>{tr.label}</option>
            ))}
          </select>
        )}

        {channelOptions.length > 0 && (
          <label className="flex items-center gap-1 text-xs text-text-secondary shrink-0">
            <span>{t(lang, 'rawDataChannel')}</span>
            <select
              value={channel}
              onChange={(e) => onChannelChange(e.target.value)}
              className="bg-bg-input border border-border rounded px-1 py-0.5 text-xs font-mono text-text-primary transition-colors hover:border-accent focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent/40 cursor-pointer max-w-[160px]"
            >
              <option value="global">{t(lang, 'rawDataGlobal')}</option>
              {channelOptions.map((o) => (
                <option key={o.key} value={o.key}>
                  {o.label ?? (o.sourceHandle || sourceLabel(o.sourceId))}
                </option>
              ))}
            </select>
          </label>
        )}

        <div className={`flex items-center gap-1 text-text-secondary text-xs font-mono ${isNum ? 'opacity-40' : ''}`}>
          <span>{totalBytes.toLocaleString()} B</span>
          {!isNum && isFiltered && (
            <span className="text-text-disabled">
              {modeCount.toLocaleString()} rows
            </span>
          )}
          {droppedBytes > 0 && (
            <span
              className="text-yellow flex items-center gap-0.5 cursor-pointer hover:underline"
              title={t(lang, 'rawDataDropped')}
              onClick={onDroppedInfoOpen}
            >
              <FileWarning size={12} />
              +{droppedBytes.toLocaleString()}
            </span>
          )}
        </div>

        <div className="flex-1" />

        {selectionCount > 0 && (
          <>
            <span className="text-text-secondary text-xs">{selectionCount}</span>
            <button
              disabled={isNum}
              className={`w-7 h-7 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-all duration-150 motion-safe:active:scale-95 cursor-pointer disabled:cursor-not-allowed disabled:pointer-events-none disabled:opacity-40 ${copyFeedback ? 'text-green' : ''}`}
              title={t(lang, 'copySelected')}
              onClick={onCopySelected}
            >
              {copyFeedback ? <Check size={14} /> : <Copy size={14} />}
            </button>
            <button
              disabled={isNum}
              className="w-7 h-7 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-all duration-150 motion-safe:active:scale-95 cursor-pointer disabled:cursor-not-allowed disabled:pointer-events-none disabled:opacity-40"
              title={t(lang, 'clearSelection')}
              onClick={onClearSelection}
            >
              <X size={14} />
            </button>
          </>
        )}

        <button
          className={`w-7 h-7 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-all duration-150 motion-safe:active:scale-95 cursor-pointer ${showTimestamp ? 'text-text-bright bg-bg-hover' : ''}`}
          title={t(lang, 'showTimestamp')}
          onClick={() => onShowTimestampChange(!showTimestamp)}
        >
          <Clock size={14} />
        </button>
        <button
          className={`w-7 h-7 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-all duration-150 motion-safe:active:scale-95 cursor-pointer ${autoScroll && !userScrolledRef.current ? 'text-text-bright bg-bg-hover' : ''}`}
          title={t(lang, 'autoScroll')}
          onClick={() => {
            onAutoScrollChange(!autoScroll);
            userScrolledRef.current = false;
          }}
        >
          <ArrowDown size={14} />
        </button>
        <button
          className={`w-7 h-7 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-all duration-150 motion-safe:active:scale-95 cursor-pointer ${showSettings ? 'text-text-bright bg-bg-hover' : ''}`}
          title={t(lang, 'settings')}
          onClick={() => onShowSettingsChange(!showSettings)}
        >
          <Settings2 size={14} />
        </button>
        <button
          className="w-7 h-7 flex items-center justify-center rounded text-text-secondary hover:bg-bg-danger hover:text-text-bright transition-all duration-150 motion-safe:active:scale-95 cursor-pointer"
          title={t(lang, 'clear')}
          onClick={onClear}
        >
          <Trash2 size={14} />
        </button>
      </div>
    </>
  );
}
