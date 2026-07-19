import { useState, useMemo, useRef, useCallback, useEffect } from 'react';
import {
  Plus,
  Trash2,
  GripVertical,
  ChevronDown,
  ChevronRight,
  Radio,
  Play,
} from 'lucide-react';
import type {
  WidgetConfig,
  DecoderBlock,
  DecoderBlockType,
  InputFormat,
  FrameDecoderManualResult,
} from '../../types';
import { useAppStore } from '../../store/appStore';
import { api } from '../../lib/tauri';
import { t } from '../../i18n';
import { nanoid } from 'nanoid';
import {
  BLOCK_TYPE_CONFIG,
  HISTORY_MAX,
  HistoryEntry,
  ExampleEntry,
  loadHistory,
  saveHistory,
  getOutputPortNames,
  blockSummary,
} from './frameDecoderShared';
import { BlockEditor } from './FrameDecoderBlockEditor';
import { LiveModePanel } from './FrameDecoderLivePanel';
import { ManualModePanel } from './FrameDecoderManualPanel';

interface FrameDecoderProps {
  widget: Extract<WidgetConfig, { kind: 'FrameDecoder' }>;
  onRemove: () => void;
}

/// 帧解码控件 — 字节流 → 按块定义解析 → 输出端口
///
/// 数据流:
///   live 模式: 后端 data_loop 喂入字节流 → FrameParser 解析 → evaluate 读取 last_frame
///              → graphOutputs[widget.id][portName] → 前端订阅显示
///   manual 模式: 用户输入 HEX/ASCII → api.parseFrameDecoderInput (临时 FrameParser.parse_once)
///                → 返回 outputs + valid + consumedBytes → 前端显示
///
/// 节点端口: 从 blocks 中 field/bitfield/length/id 块推导输出端口 (见 WidgetNode.getWidgetPorts)
export function FrameDecoder({ widget }: FrameDecoderProps) {
  const params = widget.params;
  const { id, blocks, mode } = params;
  const updateWidget = useAppStore((s) => s.updateWidget);
  const lang = useAppStore((s) => s.lang);
  const graphOutputs = useAppStore((s) => s.graphOutputs);

  // live 模式: 读取本 widget 的输出端口值
  const portNames = useMemo(() => getOutputPortNames(blocks), [blocks]);
  const liveOutputs = graphOutputs[id] ?? {};

  // manual 模式状态
  const [format, setFormat] = useState<InputFormat>('hex');
  const [input, setInput] = useState('');
  const [result, setResult] = useState<FrameDecoderManualResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [showExamples, setShowExamples] = useState(false);
  const [showHistory, setShowHistory] = useState(false);
  const examplesRef = useRef<HTMLDivElement>(null);
  const historyRef = useRef<HTMLDivElement>(null);

  // 块列表 UI 状态
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set());
  const [dragId, setDragId] = useState<string | null>(null);
  const [overId, setOverId] = useState<string | null>(null);
  const dragIdRef = useRef<string | null>(null);

  // 初始化: 加载历史
  useEffect(() => {
    setHistory(loadHistory());
  }, []);

  // 点击外部关闭下拉
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (examplesRef.current && !examplesRef.current.contains(e.target as Node)) {
        setShowExamples(false);
      }
      if (historyRef.current && !historyRef.current.contains(e.target as Node)) {
        setShowHistory(false);
      }
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, []);

  const toggleExpand = (blockId: string) => {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(blockId)) next.delete(blockId);
      else next.add(blockId);
      return next;
    });
  };

  const updateParams = (changes: Partial<typeof params>) => {
    updateWidget(id, { kind: 'FrameDecoder', params: { ...params, ...changes } });
  };

  const addBlock = (type: DecoderBlockType) => {
    const defaults: Record<DecoderBlockType, Partial<DecoderBlock>> = {
      header: { label: '', hex: 'AA' },
      length: { label: '', fieldType: 'uint8', portName: 'length', unit: 'bytes' },
      id: { label: '', fieldType: 'uint8', portName: 'id_value' },
      field: { label: '', fieldType: 'uint8', portName: `field_${portNames.length + 1}` },
      bitfield: { label: '', byteOffset: 0, bitOffset: 0, bitLength: 4, isSigned: false, portName: `bits_${portNames.length + 1}` },
      checksum: { label: '', algorithm: 'sum8', cover: 'all_prior', position: 'append' },
      tail: { label: '', hex: 'FF' },
    };
    const newBlock = { id: nanoid(6), type, ...defaults[type] } as DecoderBlock;
    updateParams({ blocks: [...blocks, newBlock] });
    setExpandedIds((prev) => new Set(prev).add(newBlock.id));
  };

  const updateBlock = (blockId: string, changes: Partial<DecoderBlock>) => {
    updateParams({
      blocks: blocks.map((b) => (b.id === blockId ? { ...b, ...changes } as DecoderBlock : b)),
    });
  };

  const removeBlock = (blockId: string) => {
    updateParams({ blocks: blocks.filter((b) => b.id !== blockId) });
    setExpandedIds((prev) => {
      const next = new Set(prev);
      next.delete(blockId);
      return next;
    });
  };

  /// 拖拽排序
  const handleDragStart = (blockId: string) => (e: React.DragEvent) => {
    e.dataTransfer.effectAllowed = 'move';
    e.dataTransfer.setData('text/plain', blockId);
    const blockEl = (e.currentTarget as HTMLElement).closest('[data-block-id]') as HTMLElement | null;
    if (blockEl) {
      e.dataTransfer.setDragImage(blockEl, 12, 12);
    }
    dragIdRef.current = blockId;
    setDragId(blockId);
  };
  const handleDragOver = (blockId: string) => (e: React.DragEvent) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
    if (dragIdRef.current && dragIdRef.current !== blockId) setOverId(blockId);
  };
  const reorderBlocks = (fromId: string, toId: string) => {
    if (fromId === toId) return;
    const fromIdx = blocks.findIndex((b) => b.id === fromId);
    const toIdx = blocks.findIndex((b) => b.id === toId);
    if (fromIdx < 0 || toIdx < 0) return;
    const next = [...blocks];
    const [moved] = next.splice(fromIdx, 1);
    next.splice(toIdx, 0, moved);
    updateParams({ blocks: next });
  };
  const handleDrop = (targetId: string) => (e: React.DragEvent) => {
    e.preventDefault();
    const draggedId = e.dataTransfer.getData('text/plain') || dragIdRef.current;
    if (!draggedId) return;
    reorderBlocks(draggedId, targetId);
    dragIdRef.current = null;
    setDragId(null);
    setOverId(null);
  };
  const handleDragEnd = () => {
    dragIdRef.current = null;
    setDragId(null);
    setOverId(null);
  };

  /// manual 模式解析
  const doParse = useCallback(async (text: string, fmt: InputFormat) => {
    if (!text.trim()) return;
    setLoading(true);
    try {
      const r = await api.parseFrameDecoderInput(
        blocks,
        text,
        fmt,
        params.enableValid,
        params.enableFrameCount,
        params.enableLastTimestamp,
        params.enableFps,
      );
      setResult(r);
      if (!r.error) {
        const entry: HistoryEntry = { input: text, format: fmt, ts: Date.now() };
        setHistory((cur) => {
          const filtered = cur.filter((h) => !(h.input === entry.input && h.format === entry.format));
          const next = [entry, ...filtered].slice(0, HISTORY_MAX);
          saveHistory(next);
          return next;
        });
      }
    } catch (e) {
      setResult({ outputs: {}, valid: false, consumedBytes: 0, error: String(e) });
    } finally {
      setLoading(false);
    }
  }, [blocks, params.enableValid, params.enableFrameCount, params.enableLastTimestamp, params.enableFps]);

  const handleParse = () => void doParse(input, format);
  const handleClear = () => { setInput(''); setResult(null); };
  const handleClearHistory = () => { setHistory([]); saveHistory([]); setShowHistory(false); };
  const handleSelectExample = (ex: ExampleEntry) => {
    setInput(ex.content); setFormat(ex.format); setShowExamples(false);
    setTimeout(() => void doParse(ex.content, ex.format), 0);
  };
  const handleSelectHistory = (h: HistoryEntry) => {
    setInput(h.input); setFormat(h.format); setShowHistory(false);
    setTimeout(() => void doParse(h.input, h.format), 0);
  };

  return (
    <div className="bg-bg-sidebar border border-border rounded flex-1 min-w-0 min-h-0 flex relative overflow-hidden">
      {/* 主区: 块列表 (可拖拽排序) */}
      <div className="flex-1 min-w-0 min-h-0 flex flex-col gap-2 p-3 overflow-y-auto bg-bg-sidebar">
        {/* 顶部: 标题 + 模式切换 */}
        <div className="flex items-center justify-between pb-1.5 border-b border-border flex-shrink-0">
          <span className="text-base font-semibold text-text-bright">{params.label}</span>
          <div className="flex items-center gap-2">
            <span className="text-[10px] text-text-secondary">{blocks.length} blocks</span>
            {/* 模式切换 */}
            <div className="flex bg-bg-input rounded border border-border overflow-hidden">
              <button
                className={`px-2 py-0.5 text-[10px] transition-colors ${mode === 'live' ? 'bg-accent text-bg-editor' : 'text-text-secondary hover:text-text-primary'}`}
                onClick={() => updateParams({ mode: 'live' })}
                title={t(lang, 'fdModeLive')}
              >
                <Radio size={10} className="inline mr-0.5" />
                {t(lang, 'fdModeLive')}
              </button>
              <button
                className={`px-2 py-0.5 text-[10px] transition-colors border-l border-border ${mode === 'manual' ? 'bg-accent text-bg-editor' : 'text-text-secondary hover:text-text-primary'}`}
                onClick={() => updateParams({ mode: 'manual' })}
                title={t(lang, 'fdModeManual')}
              >
                <Play size={10} className="inline mr-0.5" />
                {t(lang, 'fdModeManual')}
              </button>
            </div>
          </div>
        </div>

        {/* 块列表 */}
        <div
          className="flex flex-col gap-1.5"
          onDragOver={(e) => { if (dragIdRef.current) e.preventDefault(); e.dataTransfer.dropEffect = 'move'; }}
          onDrop={(e) => {
            e.preventDefault();
            const draggedId = e.dataTransfer.getData('text/plain') || dragIdRef.current;
            const targetId = overId;
            if (!draggedId || !targetId) return;
            reorderBlocks(draggedId, targetId);
            dragIdRef.current = null; setDragId(null); setOverId(null);
          }}
        >
          {blocks.length === 0 && (
            <div className="text-xs text-text-secondary opacity-60 italic py-4 text-center">
              {t(lang, 'fdBlocksEmpty')}
            </div>
          )}
          {blocks.map((block) => {
            const cfg = BLOCK_TYPE_CONFIG[block.type];
            const isExpanded = expandedIds.has(block.id);
            const isDragging = dragId === block.id;
            const isOver = overId === block.id;
            return (
              <div
                key={block.id}
                data-block-id={block.id}
                className={`border rounded-sm transition-all ${cfg.blockClass} ${isDragging ? 'opacity-40' : ''} ${isOver ? 'border-t-2 border-t-blue' : ''}`}
                onDragOver={handleDragOver(block.id)}
                onDrop={handleDrop(block.id)}
              >
                {/* 块头 */}
                <div
                  className="flex items-center gap-1.5 px-1.5 py-1 cursor-pointer select-none"
                  onClick={() => toggleExpand(block.id)}
                >
                  <div
                    className="inline-flex items-center justify-center p-0.5 cursor-grab active:cursor-grabbing text-text-secondary hover:text-text-primary flex-shrink-0"
                    title={t(lang, 'cmdDragToReorder')}
                    draggable
                    onDragStart={handleDragStart(block.id)}
                    onDragEnd={handleDragEnd}
                  >
                    <GripVertical size={12} className="pointer-events-none" />
                  </div>
                  <span
                    className={`inline-flex items-center gap-0.5 px-1 py-0.5 rounded-sm text-[9px] font-semibold uppercase tracking-wide flex-shrink-0 border ${cfg.badgeClass}`}
                  >
                    {cfg.icon}
                    {t(lang, cfg.labelKey)}
                  </span>
                  {block.label && (
                    <span className="text-xs text-text-primary truncate flex-shrink-0">{block.label}</span>
                  )}
                  <span className="text-[10px] text-text-secondary font-mono truncate flex-1 min-w-0">
                    {blockSummary(block)}
                  </span>
                  <span className="text-text-secondary flex-shrink-0 p-0.5 pointer-events-none">
                    {isExpanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
                  </span>
                  <button
                    className="text-text-secondary hover:text-red flex-shrink-0 p-0.5"
                    onClick={(e) => { e.stopPropagation(); removeBlock(block.id); }}
                    title={t(lang, 'removeWidget')}
                  >
                    <Trash2 size={11} />
                  </button>
                </div>

                {/* 块编辑区 (展开时) */}
                {isExpanded && (
                  <div className="px-2 pb-2 flex flex-col gap-1.5">
                    <BlockEditor block={block} updateBlock={updateBlock} lang={lang} />
                  </div>
                )}
              </div>
            );
          })}
        </div>

        {/* 添加块按钮 */}
        <div className="flex flex-wrap gap-1 pt-1 border-t border-border flex-shrink-0">
          {(Object.keys(BLOCK_TYPE_CONFIG) as DecoderBlockType[]).map((bt) => {
            const cfg = BLOCK_TYPE_CONFIG[bt];
            return (
              <button
                key={bt}
                className="inline-flex items-center gap-1 bg-transparent border border-dashed border-border text-text-secondary px-2 py-1 text-[11px] rounded-sm cursor-pointer transition-all hover:text-text-primary hover:border-accent"
                onClick={() => addBlock(bt)}
                title={t(lang, cfg.addLabelKey)}
              >
                <Plus size={11} />
                <span className={`inline-flex items-center gap-0.5 ${cfg.iconClass}`}>
                  {cfg.icon}
                </span>
                <span>{t(lang, cfg.addLabelKey)}</span>
              </button>
            );
          })}
        </div>
      </div>

      {/* 侧栏: 模式相关内容 + 全局设置 */}
      <div className="w-[320px] flex-shrink-0 border-l border-border bg-bg-sidebar overflow-y-auto flex flex-col gap-2 p-3">
        {mode === 'live' ? (
          <LiveModePanel
            portNames={portNames}
            liveOutputs={liveOutputs}
            enableValid={params.enableValid}
            enableFrameCount={params.enableFrameCount}
            enableLastTimestamp={params.enableLastTimestamp}
            enableFps={params.enableFps}
            lang={lang}
          />
        ) : (
          <ManualModePanel
            format={format}
            setFormat={setFormat}
            input={input}
            setInput={setInput}
            result={result}
            loading={loading}
            onParse={handleParse}
            onClear={handleClear}
            history={history}
            showExamples={showExamples}
            showHistory={showHistory}
            setShowExamples={setShowExamples}
            setShowHistory={setShowHistory}
            examplesRef={examplesRef}
            historyRef={historyRef}
            onSelectExample={handleSelectExample}
            onSelectHistory={handleSelectHistory}
            onClearHistory={handleClearHistory}
            lang={lang}
          />
        )}

        {/* 全局设置 */}
        <div className="text-[10px] text-text-secondary uppercase tracking-wide font-semibold pt-1">{t(lang, 'fdSettings')}</div>
        <div className="flex flex-col gap-1.5 p-2 bg-bg-editor border border-border rounded">
          <div className="grid grid-cols-[80px_1fr] items-center gap-2">
            <label className="text-xs text-text-secondary">{t(lang, 'cmdLabel')}</label>
            <input
              type="text"
              value={params.label}
              onChange={(e) => updateParams({ label: e.target.value })}
              className="text-xs w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded focus:outline-none focus:border-accent transition-colors"
            />
          </div>
          <label className="flex items-center gap-1.5 text-xs text-text-primary cursor-pointer">
            <input
              type="checkbox"
              checked={params.enableValid}
              onChange={(e) => updateParams({ enableValid: e.target.checked })}
              className="accent-accent"
            />
            <span>{t(lang, 'fdEnableValid')}</span>
          </label>
          <label className="flex items-center gap-1.5 text-xs text-text-primary cursor-pointer">
            <input
              type="checkbox"
              checked={params.enableFrameCount}
              onChange={(e) => updateParams({ enableFrameCount: e.target.checked })}
              className="accent-accent"
            />
            <span>{t(lang, 'fdEnableFrameCount')}</span>
          </label>
          <label className="flex items-center gap-1.5 text-xs text-text-primary cursor-pointer">
            <input
              type="checkbox"
              checked={params.enableLastTimestamp}
              onChange={(e) => updateParams({ enableLastTimestamp: e.target.checked })}
              className="accent-accent"
            />
            <span>{t(lang, 'fdEnableLastTimestamp')}</span>
          </label>
          <label className="flex items-center gap-1.5 text-xs text-text-primary cursor-pointer">
            <input
              type="checkbox"
              checked={params.enableFps}
              onChange={(e) => updateParams({ enableFps: e.target.checked })}
              className="accent-accent"
            />
            <span>{t(lang, 'fdEnableFps')}</span>
          </label>
        </div>
      </div>
    </div>
  );
}
