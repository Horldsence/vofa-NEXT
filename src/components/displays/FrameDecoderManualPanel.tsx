import { BookOpen, ChevronDown, History, Play, AlertTriangle } from 'lucide-react';
import type { InputFormat, FrameDecoderManualResult } from '../../types';
import { t } from '../../i18n';
import { FRAME_EXAMPLES } from './frameDecoderShared';
import type { HistoryEntry, ExampleEntry } from './frameDecoderShared';

export interface ManualModePanelProps {
  format: InputFormat;
  setFormat: (f: InputFormat) => void;
  input: string;
  setInput: (s: string) => void;
  result: FrameDecoderManualResult | null;
  loading: boolean;
  onParse: () => void;
  onClear: () => void;
  history: HistoryEntry[];
  showExamples: boolean;
  showHistory: boolean;
  setShowExamples: (b: boolean) => void;
  setShowHistory: (b: boolean) => void;
  examplesRef: React.RefObject<HTMLDivElement | null>;
  historyRef: React.RefObject<HTMLDivElement | null>;
  onSelectExample: (ex: ExampleEntry) => void;
  onSelectHistory: (h: HistoryEntry) => void;
  onClearHistory: () => void;
  lang: ReturnType<typeof import('../../store/appStore').useAppStore.getState>['lang'];
}

export function ManualModePanel({
  format, setFormat, input, setInput, result, loading, onParse, onClear,
  history, showExamples, showHistory, setShowExamples, setShowHistory,
  examplesRef, historyRef, onSelectExample, onSelectHistory, onClearHistory, lang,
}: ManualModePanelProps) {
  return (
    <>
      <div className="text-[10px] text-text-secondary uppercase tracking-wide font-semibold">{t(lang, 'fdManualInput')}</div>

      <div className="flex items-center gap-2 text-xs">
        <span className="text-text-secondary">{t(lang, 'fdFormat')}</span>
        <div className="flex bg-bg-input rounded border border-border overflow-hidden">
          {(['hex', 'ascii', 'auto'] as InputFormat[]).map((f) => (
            <button
              key={f}
              className={`px-2 py-0.5 transition-colors ${f !== 'hex' ? 'border-l border-border' : ''} ${format === f ? 'bg-accent text-bg-editor' : 'text-text-secondary hover:text-text-primary'}`}
              onClick={() => setFormat(f)}
            >
              {f === 'auto' ? t(lang, 'fdAutoFormat') : f.toUpperCase()}
            </button>
          ))}
        </div>
        <div className="relative" ref={examplesRef}>
          <button
            className={`flex items-center gap-1 px-2 py-0.5 rounded transition-colors ${showExamples ? 'bg-bg-hover text-text-bright' : 'text-text-secondary hover:text-text-primary hover:bg-bg-hover'}`}
            onClick={() => { setShowExamples(!showExamples); setShowHistory(false); }}
            title={t(lang, 'fdExamples')}
          >
            <BookOpen size={11} />
            <ChevronDown size={9} />
          </button>
          {showExamples && (
            <div className="absolute top-full left-0 mt-1 z-50 w-72 max-h-80 overflow-y-auto bg-bg-panel-header border border-border rounded shadow-lg">
              {FRAME_EXAMPLES.map((ex, i) => (
                <button
                  key={i}
                  className="w-full text-left px-3 py-2 hover:bg-bg-hover transition-colors border-b border-border/40 last:border-b-0"
                  onClick={() => onSelectExample(ex)}
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-xs text-text-bright truncate">{ex.name}</span>
                    <span className="text-[10px] text-text-secondary uppercase flex-shrink-0">{ex.format}</span>
                  </div>
                  <div className="text-[10px] text-text-secondary truncate mt-0.5">{ex.description}</div>
                </button>
              ))}
            </div>
          )}
        </div>
        <div className="relative" ref={historyRef}>
          <button
            className={`flex items-center gap-1 px-2 py-0.5 rounded transition-colors ${showHistory ? 'bg-bg-hover text-text-bright' : 'text-text-secondary hover:text-text-primary hover:bg-bg-hover'}`}
            onClick={() => { setShowHistory(!showHistory); setShowExamples(false); }}
            title={t(lang, 'fdHistory')}
          >
            <History size={11} />
            <ChevronDown size={9} />
          </button>
          {showHistory && (
            <div className="absolute top-full left-0 mt-1 z-50 w-72 max-h-80 overflow-y-auto bg-bg-panel-header border border-border rounded shadow-lg">
              {history.length === 0 ? (
                <div className="px-3 py-2 text-xs text-text-secondary italic">{t(lang, 'fdHistoryEmpty')}</div>
              ) : (
                <>
                  {history.map((h, i) => (
                    <button
                      key={i}
                      className="w-full text-left px-3 py-1.5 hover:bg-bg-hover transition-colors border-b border-border/40 last:border-b-0"
                      onClick={() => onSelectHistory(h)}
                    >
                      <div className="text-[10px] text-text-secondary uppercase flex-shrink-0">{h.format}</div>
                      <div className="text-xs text-text-primary font-mono truncate">{h.input}</div>
                    </button>
                  ))}
                  <button
                    className="w-full text-left px-3 py-1.5 text-xs text-red hover:bg-bg-hover transition-colors"
                    onClick={onClearHistory}
                  >
                    {t(lang, 'fdClearHistory')}
                  </button>
                </>
              )}
            </div>
          )}
        </div>
      </div>

      <textarea
        className="w-full font-mono text-xs bg-bg-input text-text-primary border border-border rounded-sm px-2 py-1.5 outline-none focus:border-accent resize-y min-h-[60px] leading-relaxed"
        value={input}
        onChange={(e) => setInput(e.target.value)}
        placeholder={format === 'hex' ? t(lang, 'fdHexPlaceholder') : t(lang, 'fdAsciiPlaceholder')}
        spellCheck={false}
        rows={3}
        onKeyDown={(e) => {
          if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); onParse(); }
        }}
      />
      <div className="text-[10px] text-text-secondary opacity-70">{t(lang, 'fdShortcutHint')}</div>

      <div className="flex gap-1.5">
        <button
          className="flex-1 justify-center px-3 py-1.5 bg-bg-button text-text-inverse border-none rounded cursor-pointer text-sm transition-colors hover:bg-bg-button-hover font-semibold inline-flex items-center gap-1.5 disabled:opacity-50 disabled:cursor-default"
          onClick={onParse}
          disabled={loading || !input.trim()}
        >
          <Play size={12} />
          <span>{t(lang, 'fdParse')}</span>
        </button>
        <button
          className="px-3 py-1.5 bg-transparent text-text-secondary border border-border rounded cursor-pointer text-xs hover:text-text-primary hover:border-accent transition-colors"
          onClick={onClear}
        >
          {t(lang, 'fdClear')}
        </button>
      </div>

      {result && (
        <div className="flex flex-col gap-1.5">
          <div className="text-[10px] text-text-secondary uppercase tracking-wide font-semibold">{t(lang, 'fdParseResult')}</div>
          {result.error ? (
            <div className="flex items-start gap-1 bg-red/10 border border-red/30 text-red px-2 py-1.5 rounded-sm text-xs">
              <AlertTriangle size={11} className="flex-shrink-0 mt-0.5" />
              <span className="break-all">{result.error}</span>
            </div>
          ) : (
            <>
              <div className="flex items-center gap-2 px-2 py-1 bg-bg-editor rounded-sm">
                <span className="text-[10px] text-text-secondary">{t(lang, 'fdResultValid')}</span>
                <span className={`text-xs font-mono ${result.valid ? 'text-green' : 'text-red'}`}>
                  {result.valid ? '✓ PASS' : '✗ FAIL'}
                </span>
                <span className="text-[10px] text-text-secondary ml-auto">
                  {t(lang, 'fdConsumedBytes')}: <span className="font-mono text-text-primary">{result.consumedBytes}</span>
                </span>
              </div>
              <div className="bg-bg-editor border border-border rounded p-2 flex flex-col gap-1">
                {Object.keys(result.outputs).length === 0 ? (
                  <div className="text-xs text-text-secondary opacity-60 italic py-1 text-center">{t(lang, 'fdNoOutputs')}</div>
                ) : (
                  Object.entries(result.outputs).map(([port, val]) => (
                    <div key={port} className="flex items-center justify-between gap-2 px-1.5 py-0.5 bg-bg-editor rounded-sm">
                      <span className="text-[10px] text-text-secondary font-mono">{port}</span>
                      <span className="text-xs font-mono text-green">{val.toFixed(4)}</span>
                    </div>
                  ))
                )}
              </div>
            </>
          )}
        </div>
      )}
    </>
  );
}
