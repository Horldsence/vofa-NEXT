import { useState, useMemo, useRef, useCallback, useEffect } from 'react';
import {
  Plus,
  Trash2,
  AlertTriangle,
  Hexagon,
  Variable,
  Hash,
  ShieldCheck,
  GripVertical,
  ChevronDown,
  ChevronRight,
  Flag,
  Fingerprint,
  Grid3x3,
  BookOpen,
  History,
  Play,
  Radio,
} from 'lucide-react';
import type {
  WidgetConfig,
  DecoderBlock,
  DecoderBlockType,
  DecoderChecksumPosition,
  DecoderChecksumCover,
  ChecksumType,
  FieldType,
  InputFormat,
  FrameDecoderManualResult,
} from '../../types';
import { useAppStore } from '../../store/appStore';
import { api } from '../../lib/tauri';
import { t } from '../../i18n';
import { nanoid } from 'nanoid';

interface FrameDecoderProps {
  widget: Extract<WidgetConfig, { kind: 'FrameDecoder' }>;
  onRemove: () => void;
}

/// 块类型配置: 图标 / Tailwind 静态类名 / 标签 key
/// 颜色全部使用 @theme 内已定义的语义色 (blue/yellow/purple/green/orange/red/accent)
const BLOCK_TYPE_CONFIG: Record<
  DecoderBlockType,
  {
    icon: React.ReactNode;
    badgeClass: string;   // 标签: bg-{c}/20 text-{c} border-{c}/40
    blockClass: string;   // 块容器: border-{c}/40 bg-{c}/10
    iconClass: string;    // 图标: text-{c}
    labelKey: string;
    addLabelKey: string;
  }
> = {
  header:   { icon: <Hexagon size={12} />,     badgeClass: 'bg-blue/20 text-blue border-blue/40',         blockClass: 'border-blue/40 bg-blue/10',         iconClass: 'text-blue',   labelKey: 'fdBlockHeader',   addLabelKey: 'fdAddBlockHeader' },
  length:   { icon: <Hash size={12} />,        badgeClass: 'bg-yellow/20 text-yellow border-yellow/40',   blockClass: 'border-yellow/40 bg-yellow/10',     iconClass: 'text-yellow', labelKey: 'fdBlockLength',   addLabelKey: 'fdAddBlockLength' },
  id:       { icon: <Fingerprint size={12} />, badgeClass: 'bg-purple/20 text-purple border-purple/40',   blockClass: 'border-purple/40 bg-purple/10',     iconClass: 'text-purple', labelKey: 'fdBlockId',       addLabelKey: 'fdAddBlockId' },
  field:    { icon: <Variable size={12} />,    badgeClass: 'bg-green/20 text-green border-green/40',     blockClass: 'border-green/40 bg-green/10',       iconClass: 'text-green',  labelKey: 'fdBlockField',    addLabelKey: 'fdAddBlockField' },
  bitfield: { icon: <Grid3x3 size={12} />,     badgeClass: 'bg-orange/20 text-orange border-orange/40',  blockClass: 'border-orange/40 bg-orange/10',     iconClass: 'text-orange', labelKey: 'fdBlockBitfield', addLabelKey: 'fdAddBlockBitfield' },
  checksum: { icon: <ShieldCheck size={12} />, badgeClass: 'bg-red/20 text-red border-red/40',           blockClass: 'border-red/40 bg-red/10',           iconClass: 'text-red',    labelKey: 'fdBlockChecksum', addLabelKey: 'fdAddBlockChecksum' },
  tail:     { icon: <Flag size={12} />,        badgeClass: 'bg-accent/20 text-accent border-accent/40',  blockClass: 'border-accent/40 bg-accent/10',     iconClass: 'text-accent', labelKey: 'fdBlockTail',     addLabelKey: 'fdAddBlockTail' },
};

const CHECKSUM_OPTIONS: { value: ChecksumType; labelKey: string }[] = [
  { value: 'none', labelKey: 'cmdChecksumNone' },
  { value: 'sum8', labelKey: 'cmdChecksumSum8' },
  { value: 'xor8', labelKey: 'cmdChecksumXor8' },
  { value: 'crc8', labelKey: 'cmdChecksumCRC8' },
  { value: 'crc16Modbus', labelKey: 'cmdChecksumCRC16Modbus' },
  { value: 'crc16CCITT', labelKey: 'cmdChecksumCRC16CCITT' },
  { value: 'crc32', labelKey: 'cmdChecksumCRC32' },
  { value: 'lrc', labelKey: 'cmdChecksumLRC' },
  { value: 'custom', labelKey: 'cmdChecksumCustom' },
];

const FIELD_TYPE_OPTIONS: FieldType[] = [
  'uint8', 'int8',
  'uint16LE', 'uint16BE', 'int16LE', 'int16BE',
  'uint32LE', 'uint32BE', 'int32LE', 'int32BE',
  'float32LE', 'float32BE',
  'bytes',
];

const CHECKSUM_POSITION_OPTIONS: { value: DecoderChecksumPosition; labelKey: string }[] = [
  { value: 'append', labelKey: 'fdChecksumPosAppend' },
  { value: 'inline', labelKey: 'fdChecksumPosInline' },
  { value: 'prepend', labelKey: 'fdChecksumPosPrepend' },
];

const CHECKSUM_COVER_OPTIONS: { value: DecoderChecksumCover; labelKey: string }[] = [
  { value: 'all_prior', labelKey: 'fdChecksumCoverAllPrior' },
  { value: 'range', labelKey: 'fdChecksumCoverRange' },
];

/// 历史记录最大条数
const HISTORY_MAX = 20;
/// localStorage key
const HISTORY_KEY = 'frame-decoder-history-v1';

interface HistoryEntry {
  input: string;
  format: InputFormat;
  ts: number;
}

/// 示例模板条目 (帧解码器专用, 覆盖典型帧格式)
interface ExampleEntry {
  name: string;
  description: string;
  format: InputFormat;
  content: string;
}

const FRAME_EXAMPLES: ExampleEntry[] = [
  {
    name: '定长帧 · header + 2 字段 + sum8',
    description: 'AA 01 02 03 (header=AA, field1=01, field2=02, sum=03)',
    format: 'hex',
    content: 'AA 01 02 03',
  },
  {
    name: '定长帧 · 多帧连续',
    description: 'AA 01 02 03 AA 04 05 09',
    format: 'hex',
    content: 'AA 01 02 03 AA 04 05 09',
  },
  {
    name: '变长帧 · header + length + payload + tail',
    description: 'AA 03 11 22 33 FF (length=3, payload=11 22 33, tail=FF)',
    format: 'hex',
    content: 'AA 03 11 22 33 FF',
  },
  {
    name: '多帧分派 · id 区分帧类型',
    description: 'AA 01 10 20 / AA 02 30 40 50 (id=1 短帧, id=2 长帧)',
    format: 'hex',
    content: 'AA 01 10 20 AA 02 30 40 50',
  },
  {
    name: '带校验 · CRC16-Modbus',
    description: 'AA 01 02 03 04 (含 CRC16, 末尾 2 字节小端)',
    format: 'hex',
    content: 'AA 01 02 0B 6B',
  },
  {
    name: '位域字段 · 单字节多字段',
    description: 'AA 5A 00 (5A = 0101 1010, 高 4 位=5, 低 4 位=A)',
    format: 'hex',
    content: 'AA 5A 00',
  },
];

function loadHistory(): HistoryEntry[] {
  try {
    const raw = localStorage.getItem(HISTORY_KEY);
    if (!raw) return [];
    const arr = JSON.parse(raw);
    if (!Array.isArray(arr)) return [];
    return arr.filter(
      (e) => e && typeof e.input === 'string' && typeof e.format === 'string'
    );
  } catch {
    return [];
  }
}

function saveHistory(entries: HistoryEntry[]) {
  try {
    localStorage.setItem(HISTORY_KEY, JSON.stringify(entries));
  } catch {
    // 忽略配额错误
  }
}

/// 块摘要 (列表中单行显示)
function blockSummary(block: DecoderBlock): string {
  switch (block.type) {
    case 'header':
      return block.hex ?? '';
    case 'length':
      return `${block.portName ?? 'length'} : ${block.fieldType ?? 'uint8'}`;
    case 'id':
      return `${block.portName ?? 'id_value'} : ${block.fieldType ?? 'uint8'}`;
    case 'field':
      return `${block.portName ?? ''} : ${block.fieldType ?? 'uint8'}`;
    case 'bitfield':
      return `${block.portName ?? ''} @${block.byteOffset}.${block.bitOffset}:${block.bitLength}${block.isSigned ? 's' : 'u'}`;
    case 'checksum':
      return block.algorithm ?? 'sum8';
    case 'tail':
      return block.hex ?? '';
  }
}

/// 从 blocks 推导输出端口名 (用于 live 模式显示)
function getOutputPortNames(blocks: DecoderBlock[]): string[] {
  const names: string[] = [];
  for (const b of blocks) {
    if (b.type === 'length') names.push(b.portName ?? 'length');
    else if (b.type === 'id') names.push(b.portName ?? 'id_value');
    else if (b.type === 'field' || b.type === 'bitfield') names.push(b.portName);
  }
  return names;
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

// ============ 块编辑器 (展开时显示) ============

interface BlockEditorProps {
  block: DecoderBlock;
  updateBlock: (id: string, changes: Partial<DecoderBlock>) => void;
  lang: ReturnType<typeof useAppStore.getState>['lang'];
}

function BlockEditor({ block, updateBlock, lang }: BlockEditorProps) {
  const inputClass = "w-full px-1.5 py-0.5 bg-bg-input text-text-primary border border-border rounded-sm text-xs font-mono focus:outline-none focus:border-accent";
  const labelClass = "text-[10px] text-text-secondary";
  const selectClass = "w-full px-1.5 py-0.5 bg-bg-input text-text-primary border border-border rounded-sm text-xs focus:outline-none focus:border-accent";

  return (
    <>
      {/* 通用: label */}
      <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
        <label className={labelClass}>{t(lang, 'cmdBlockLabel')}</label>
        <input
          type="text"
          value={block.label ?? ''}
          onChange={(e) => updateBlock(block.id, { label: e.target.value } as Partial<DecoderBlock>)}
          className={inputClass}
          placeholder={t(lang, 'cmdBlockLabelPlaceholder')}
        />
      </div>

      {/* header / tail: HEX */}
      {(block.type === 'header' || block.type === 'tail') && (
        <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
          <label className={labelClass}>HEX</label>
          <input
            type="text"
            value={block.hex ?? ''}
            onChange={(e) => updateBlock(block.id, { hex: e.target.value } as Partial<DecoderBlock>)}
            className={inputClass}
            placeholder="AA BB"
            spellCheck={false}
          />
        </div>
      )}

      {/* length / id / field: fieldType + portName */}
      {(block.type === 'length' || block.type === 'id' || block.type === 'field') && (
        <>
          <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
            <label className={labelClass}>{t(lang, 'cmdBlockType')}</label>
            <select
              value={block.fieldType ?? 'uint8'}
              onChange={(e) => updateBlock(block.id, { fieldType: e.target.value as FieldType } as Partial<DecoderBlock>)}
              className={selectClass}
            >
              {FIELD_TYPE_OPTIONS.map((ft) => (
                <option key={ft} value={ft}>{ft}</option>
              ))}
            </select>
          </div>
          <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
            <label className={labelClass}>{t(lang, 'cmdBlockPortName')}</label>
            <input
              type="text"
              value={block.portName ?? ''}
              onChange={(e) => updateBlock(block.id, { portName: e.target.value } as Partial<DecoderBlock>)}
              className={inputClass}
              placeholder={block.type === 'length' ? 'length' : block.type === 'id' ? 'id_value' : 'field_1'}
              spellCheck={false}
            />
          </div>
        </>
      )}

      {/* length: unit */}
      {block.type === 'length' && (
        <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
          <label className={labelClass}>{t(lang, 'fdLengthUnit')}</label>
          <select
            value={block.unit ?? 'bytes'}
            onChange={(e) => updateBlock(block.id, { unit: e.target.value as 'bytes' | 'fields' } as Partial<DecoderBlock>)}
            className={selectClass}
          >
            <option value="bytes">{t(lang, 'fdLengthUnitBytes')}</option>
            <option value="fields">{t(lang, 'fdLengthUnitFields')}</option>
          </select>
        </div>
      )}

      {/* field: lengthRef (仅 bytes 类型有意义) */}
      {block.type === 'field' && (
        <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
          <label className={labelClass}>{t(lang, 'fdLengthRef')}</label>
          <input
            type="text"
            value={block.lengthRef ?? ''}
            onChange={(e) => updateBlock(block.id, { lengthRef: e.target.value || null } as Partial<DecoderBlock>)}
            className={inputClass}
            placeholder={t(lang, 'fdLengthRefPlaceholder')}
            spellCheck={false}
          />
        </div>
      )}

      {/* bitfield: byteOffset / bitOffset / bitLength / isSigned / portName */}
      {block.type === 'bitfield' && (
        <>
          <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
            <label className={labelClass}>{t(lang, 'fdByteOffset')}</label>
            <input
              type="number"
              min={0}
              value={block.byteOffset ?? 0}
              onChange={(e) => updateBlock(block.id, { byteOffset: parseInt(e.target.value, 10) || 0 } as Partial<DecoderBlock>)}
              className={inputClass}
            />
          </div>
          <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
            <label className={labelClass}>{t(lang, 'fdBitOffset')}</label>
            <input
              type="number"
              min={0}
              max={7}
              value={block.bitOffset ?? 0}
              onChange={(e) => updateBlock(block.id, { bitOffset: parseInt(e.target.value, 10) || 0 } as Partial<DecoderBlock>)}
              className={inputClass}
            />
          </div>
          <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
            <label className={labelClass}>{t(lang, 'fdBitLength')}</label>
            <input
              type="number"
              min={1}
              max={32}
              value={block.bitLength ?? 4}
              onChange={(e) => updateBlock(block.id, { bitLength: parseInt(e.target.value, 10) || 1 } as Partial<DecoderBlock>)}
              className={inputClass}
            />
          </div>
          <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
            <label className={labelClass}>{t(lang, 'fdSigned')}</label>
            <button
              className={`px-2 py-0.5 text-xs rounded-sm border ${block.isSigned ? 'bg-bg-button text-text-inverse border-bg-button' : 'bg-bg-input text-text-secondary border-border'}`}
              onClick={() => updateBlock(block.id, { isSigned: !block.isSigned } as Partial<DecoderBlock>)}
            >
              {block.isSigned ? t(lang, 'fdSignedYes') : t(lang, 'fdSignedNo')}
            </button>
          </div>
          <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
            <label className={labelClass}>{t(lang, 'cmdBlockPortName')}</label>
            <input
              type="text"
              value={block.portName ?? ''}
              onChange={(e) => updateBlock(block.id, { portName: e.target.value } as Partial<DecoderBlock>)}
              className={inputClass}
              placeholder="bits_1"
              spellCheck={false}
            />
          </div>
        </>
      )}

      {/* checksum: algorithm / cover / position / customScript */}
      {block.type === 'checksum' && (
        <>
          <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
            <label className={labelClass}>{t(lang, 'cmdChecksum')}</label>
            <select
              value={block.algorithm ?? 'sum8'}
              onChange={(e) => updateBlock(block.id, { algorithm: e.target.value as ChecksumType } as Partial<DecoderBlock>)}
              className={selectClass}
            >
              {CHECKSUM_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {t(lang, opt.labelKey)}
                </option>
              ))}
            </select>
          </div>
          <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
            <label className={labelClass}>{t(lang, 'fdChecksumCover')}</label>
            <select
              value={block.cover ?? 'all_prior'}
              onChange={(e) => updateBlock(block.id, { cover: e.target.value as DecoderChecksumCover } as Partial<DecoderBlock>)}
              className={selectClass}
            >
              {CHECKSUM_COVER_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {t(lang, opt.labelKey)}
                </option>
              ))}
            </select>
          </div>
          {block.cover === 'range' && (
            <>
              <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
                <label className={labelClass}>{t(lang, 'fdCoverStart')}</label>
                <input
                  type="number"
                  min={0}
                  value={block.coverStart ?? 0}
                  onChange={(e) => updateBlock(block.id, { coverStart: parseInt(e.target.value, 10) || 0 } as Partial<DecoderBlock>)}
                  className={inputClass}
                />
              </div>
              <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
                <label className={labelClass}>{t(lang, 'fdCoverEnd')}</label>
                <input
                  type="number"
                  min={0}
                  value={block.coverEnd ?? 0}
                  onChange={(e) => updateBlock(block.id, { coverEnd: parseInt(e.target.value, 10) || 0 } as Partial<DecoderBlock>)}
                  className={inputClass}
                />
              </div>
            </>
          )}
          <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
            <label className={labelClass}>{t(lang, 'cmdChecksumPosition')}</label>
            <select
              value={block.position ?? 'append'}
              onChange={(e) => updateBlock(block.id, { position: e.target.value as DecoderChecksumPosition } as Partial<DecoderBlock>)}
              className={selectClass}
            >
              {CHECKSUM_POSITION_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {t(lang, opt.labelKey)}
                </option>
              ))}
            </select>
          </div>
          {block.algorithm === 'custom' && (
            <div className="flex flex-col gap-1">
              <label className={labelClass}>{t(lang, 'cmdCustomScript')}</label>
              <textarea
                className="w-full font-mono text-xs bg-bg-input text-text-primary border border-border rounded-sm px-1.5 py-1 outline-none focus:border-accent resize-y min-h-[60px] leading-relaxed"
                value={block.customScript ?? ''}
                onChange={(e) => updateBlock(block.id, { customScript: e.target.value } as Partial<DecoderBlock>)}
                spellCheck={false}
                rows={4}
                placeholder={'// bytes: 输入字节数组\n// 返回: 校验字节数组\nlet s = 0;\nfor (const b of bytes) s = (s + b) & 0xff;\nreturn [s];'}
              />
              <div className="flex items-center gap-1 px-1.5 py-0.5 bg-yellow/10 border border-yellow/30 text-yellow text-[10px] rounded-sm">
                <AlertTriangle size={10} />
                <span>{t(lang, 'cmdCustomWarn')}</span>
              </div>
            </div>
          )}
        </>
      )}

      {/* matchId (除 id 块外都可设置) */}
      {block.type !== 'id' && (
        <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
          <label className={labelClass}>{t(lang, 'fdMatchId')}</label>
          <input
            type="number"
            value={block.matchId ?? ''}
            onChange={(e) => {
              const v = e.target.value === '' ? null : parseInt(e.target.value, 10);
              updateBlock(block.id, { matchId: v } as Partial<DecoderBlock>);
            }}
            className={inputClass}
            placeholder={t(lang, 'fdMatchIdPlaceholder')}
          />
        </div>
      )}
    </>
  );
}

// ============ Live 模式面板 ============

interface LiveModePanelProps {
  portNames: string[];
  liveOutputs: Record<string, number>;
  enableValid: boolean;
  enableFrameCount: boolean;
  enableLastTimestamp: boolean;
  enableFps: boolean;
  lang: ReturnType<typeof useAppStore.getState>['lang'];
}

function LiveModePanel({ portNames, liveOutputs, enableValid, enableFrameCount, enableLastTimestamp, enableFps, lang }: LiveModePanelProps) {
  const allPorts = useMemo(() => {
    const ports = [...portNames];
    if (enableValid) ports.push('valid');
    if (enableFrameCount) ports.push('frame_count');
    if (enableLastTimestamp) ports.push('last_timestamp');
    if (enableFps) ports.push('fps');
    return ports;
  }, [portNames, enableValid, enableFrameCount, enableLastTimestamp, enableFps]);

  return (
    <>
      <div className="text-[10px] text-text-secondary uppercase tracking-wide font-semibold">{t(lang, 'fdLiveOutputs')}</div>
      <div className="bg-bg-editor border border-border rounded p-2 flex flex-col gap-1">
        {allPorts.length === 0 ? (
          <div className="text-xs text-text-secondary opacity-60 italic py-2 text-center">{t(lang, 'fdNoPorts')}</div>
        ) : (
          allPorts.map((port) => {
            const val = liveOutputs[port];
            const hasVal = typeof val === 'number';
            return (
              <div key={port} className="flex items-center justify-between gap-2 px-1.5 py-0.5 bg-bg-editor rounded-sm">
                <span className="text-[10px] text-text-secondary font-mono">{port}</span>
                <span className={`text-xs font-mono ${hasVal ? 'text-green' : 'text-text-secondary opacity-60'}`}>
                  {hasVal ? val.toFixed(4) : '—'}
                </span>
              </div>
            );
          })
        )}
      </div>
      <div className="text-[10px] text-text-secondary opacity-70 px-1">
        {t(lang, 'fdLiveHint')}
      </div>
    </>
  );
}

// ============ Manual 模式面板 ============

interface ManualModePanelProps {
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
  lang: ReturnType<typeof useAppStore.getState>['lang'];
}

function ManualModePanel({
  format, setFormat, input, setInput, result, loading, onParse, onClear,
  history, showExamples, showHistory, setShowExamples, setShowHistory,
  examplesRef, historyRef, onSelectExample, onSelectHistory, onClearHistory, lang,
}: ManualModePanelProps) {
  return (
    <>
      <div className="text-[10px] text-text-secondary uppercase tracking-wide font-semibold">{t(lang, 'fdManualInput')}</div>

      {/* 格式选择 + 示例 + 历史 */}
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

      {/* 输入框 */}
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

      {/* 解析按钮 */}
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

      {/* 解析结果 */}
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
