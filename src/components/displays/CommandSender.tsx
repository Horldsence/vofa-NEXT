import { useState, useMemo, useRef } from 'react';
import {
  Send,
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
} from 'lucide-react';
import type {
  WidgetConfig,
  BlockType,
  CommandBlock,
  ChecksumType,
  FieldType,
} from '../../types';
import { useAppStore } from '../../store/appStore';
import { useGraphInputs } from '../../lib/useGraphInput';
import { computeChecksum, type ChecksumKind } from '../../lib/checksum';
import { parseHex, packField, bytesToHex, bytesToAscii } from '../../lib/commandParser';
import { t } from '../../i18n';
import { nanoid } from 'nanoid';

interface CommandSenderProps {
  widget: Extract<WidgetConfig, { kind: 'Command' }>;
  onRemove: () => void;
}

/// 块类型配置: 图标 / Tailwind 静态类名 / 标签 key
/// 颜色全部使用 @theme 内已定义的语义色 (blue/green/yellow/red)
const BLOCK_TYPE_CONFIG: Record<
  BlockType,
  {
    icon: React.ReactNode;
    badgeClass: string;   // 标签: bg-{c}/20 text-{c} border-{c}/40
    blockClass: string;   // 块容器: border-{c}/40 bg-{c}/10
    iconClass: string;    // 图标: text-{c}
    labelKey: string;
    addLabelKey: string;
  }
> = {
  const_hex:   { icon: <Hexagon size={12} />,     badgeClass: 'bg-blue/20 text-blue border-blue/40',       blockClass: 'border-blue/40 bg-blue/10',       iconClass: 'text-blue',   labelKey: 'cmdBlockConstHex',   addLabelKey: 'cmdAddBlockConstHex' },
  var_ref:     { icon: <Variable size={12} />,    badgeClass: 'bg-green/20 text-green border-green/40',   blockClass: 'border-green/40 bg-green/10',     iconClass: 'text-green',  labelKey: 'cmdBlockVarRef',     addLabelKey: 'cmdAddBlockVarRef' },
  typed_const: { icon: <Hash size={12} />,        badgeClass: 'bg-yellow/20 text-yellow border-yellow/40', blockClass: 'border-yellow/40 bg-yellow/10',   iconClass: 'text-yellow', labelKey: 'cmdBlockTypedConst', addLabelKey: 'cmdAddBlockTypedConst' },
  checksum:    { icon: <ShieldCheck size={12} />, badgeClass: 'bg-red/20 text-red border-red/40',         blockClass: 'border-red/40 bg-red/10',         iconClass: 'text-red',    labelKey: 'cmdBlockChecksum',   addLabelKey: 'cmdAddBlockChecksum' },
};

const CHECKSUM_OPTIONS: { value: ChecksumType; labelKey: string }[] = [
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

/// 拼接多个 Uint8Array
function concatChunks(chunks: Uint8Array[]): Uint8Array {
  const total = chunks.reduce((s, c) => s + c.length, 0);
  const result = new Uint8Array(total);
  let offset = 0;
  for (const c of chunks) {
    result.set(c, offset);
    offset += c.length;
  }
  return result;
}

/// 块摘要 (列表中单行显示)
function blockSummary(block: CommandBlock): string {
  switch (block.type) {
    case 'const_hex':
      return block.hex ?? '';
    case 'var_ref':
      return `${block.portName ?? 'value'} : ${block.fieldType ?? 'uint16LE'}`;
    case 'typed_const':
      return `${block.value ?? '0'} : ${block.fieldType ?? 'uint8'}`;
    case 'checksum':
      return block.checksum ?? 'sum8';
  }
}

/// 命令发送控件 — 数据块拼接方式
///
/// 数据流:
///   1. blocks 列表按顺序逐块编码 → 拼接为 payload
///   2. var_ref 块从 useGraphInputs 读取连入值 (端口名自定义)
///   3. checksum 块对前面所有块的累计字节计算校验
///   4. 追加 \n (可选)
///   5. store.sendData(byteArray) → 后端 send_raw → transport
///
/// 节点端口: 从 blocks 中 var_ref 块的 portName 动态推导 (见 WidgetNode.getWidgetPorts)
export function CommandSender({ widget }: CommandSenderProps) {
  const params = widget.params;
  const { id, blocks } = params;
  const updateWidget = useAppStore((s) => s.updateWidget);
  const sendData = useAppStore((s) => s.sendData);
  const lang = useAppStore((s) => s.lang);

  // 从 var_ref 块推导输入端口名, 读取连入值
  const portNames = useMemo(
    () => blocks.filter((b) => b.type === 'var_ref' && b.portName).map((b) => b.portName!),
    [blocks]
  );
  const graphInputs = useGraphInputs(id, portNames, 0);

  const [error, setError] = useState<string | null>(null);
  const [lastSent, setLastSent] = useState<string | null>(null);
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set());
  const [dragId, setDragId] = useState<string | null>(null);
  const [overId, setOverId] = useState<string | null>(null);
  const sendCountRef = useRef(0);
  // dragId 同步 ref: 避免 setDragId 异步导致 handleDrop 闭包中 dragId 过时
  const dragIdRef = useRef<string | null>(null);

  const toggleExpand = (blockId: string) => {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(blockId)) next.delete(blockId);
      else next.add(blockId);
      return next;
    });
  };

  /// 计算最终字节流 (blocks 拼接 + 可选 \n)
  const computed = useMemo<{ bytes: Uint8Array | null; error: string | null; perBlock: Uint8Array[][] }>(() => {
    try {
      const chunks: Uint8Array[] = [];
      const perBlock: Uint8Array[][] = [];
      for (const block of blocks) {
        let chunk: Uint8Array;
        switch (block.type) {
          case 'const_hex':
            chunk = parseHex(block.hex ?? '');
            break;
          case 'var_ref': {
            const val = graphInputs[block.portName ?? 'value'] ?? 0;
            chunk = packField(block.fieldType ?? 'uint16LE', String(val));
            break;
          }
          case 'typed_const':
            chunk = packField(block.fieldType ?? 'uint8', block.value ?? '0');
            break;
          case 'checksum': {
            const prev = concatChunks(chunks);
            chunk = new Uint8Array(computeChecksum(
              prev,
              (block.checksum ?? 'sum8') as ChecksumKind,
              block.checksum === 'custom' ? block.customScript : undefined
            ));
            break;
          }
        }
        chunks.push(chunk);
        perBlock.push([chunk]);
      }
      let result = concatChunks(chunks);
      if (params.appendNewline) {
        const withNl = new Uint8Array(result.length + 1);
        withNl.set(result, 0);
        withNl[result.length] = 0x0a;
        result = withNl;
      }
      return { bytes: result, error: null, perBlock };
    } catch (e) {
      return { bytes: null, error: (e as Error).message, perBlock: [] };
    }
  }, [blocks, graphInputs, params.appendNewline]);

  const handleSend = async () => {
    setError(null);
    if (!computed.bytes || computed.bytes.length === 0) {
      setError(t(lang, 'cmdErrorEmpty'));
      return;
    }
    try {
      await sendData(Array.from(computed.bytes));
      sendCountRef.current += 1;
      setLastSent(`${new Date().toLocaleTimeString()} #${sendCountRef.current} [${computed.bytes.length}B] ${bytesToHex(computed.bytes)}`);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const updateParams = (changes: Partial<typeof params>) => {
    updateWidget(id, { kind: 'Command', params: { ...params, ...changes } });
  };

  /// 添加块
  const addBlock = (type: BlockType) => {
    const defaults: Record<BlockType, Partial<CommandBlock>> = {
      const_hex: { label: '', hex: '00' },
      var_ref: { label: '', portName: `in${portNames.length + 1}`, fieldType: 'uint16LE' },
      typed_const: { label: '', fieldType: 'uint8', value: '0' },
      checksum: { label: '', checksum: 'sum8' },
    };
    const newBlock: CommandBlock = { id: nanoid(6), type, ...defaults[type] };
    updateParams({ blocks: [...blocks, newBlock] });
    setExpandedIds((prev) => new Set(prev).add(newBlock.id));
  };

  /// 更新块
  const updateBlock = (blockId: string, changes: Partial<CommandBlock>) => {
    updateParams({
      blocks: blocks.map((b) => (b.id === blockId ? { ...b, ...changes } : b)),
    });
  };

  /// 删除块
  const removeBlock = (blockId: string) => {
    updateParams({ blocks: blocks.filter((b) => b.id !== blockId) });
    setExpandedIds((prev) => {
      const next = new Set(prev);
      next.delete(blockId);
      return next;
    });
  };

  /// 拖拽排序 — 通过 dataTransfer 传递 dragId, 避免闭包过时
  /// setDragImage: 让整个块卡片作为拖动图像 (而非仅手柄), 视觉反馈更清晰
  /// dragIdRef: 同步存储 dragId, 确保 handleDrop 能读到最新值
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
  /// 执行数组重排 (从 fromId 移动到 toId 之前)
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

  return (
    <div className="bg-bg-sidebar border border-border rounded flex-1 min-w-0 min-h-0 flex relative overflow-hidden">
      {/* 主区: 块列表 (可拖拽排序) */}
      <div className="flex-1 min-w-0 min-h-0 flex flex-col gap-2 p-3 overflow-y-auto bg-bg-sidebar">
        <div className="flex items-center justify-between pb-1.5 border-b border-border flex-shrink-0">
          <span className="text-base font-semibold text-text-bright">{params.label}</span>
          <span className="text-[10px] text-text-secondary">{blocks.length} blocks</span>
        </div>

        {/* 块列表 — 不用 flex-1/min-h-0, 让其按内容自然撑高, 由主区 overflow-y-auto 滚动
            onDragOver/onDrop fallback: 块卡片之间 gap 区域也能触发 drop, 用 overId 作为目标 */}
        <div
          className="flex flex-col gap-1.5"
          onDragOver={(e) => {
            // gap 区域也允许 drop (overId 由块卡片的 dragover 设置)
            if (dragIdRef.current) e.preventDefault();
            e.dataTransfer.dropEffect = 'move';
          }}
          onDrop={(e) => {
            e.preventDefault();
            const draggedId = e.dataTransfer.getData('text/plain') || dragIdRef.current;
            // drop 到 gap: 用最近的 overId 作为目标
            const targetId = overId;
            if (!draggedId || !targetId) return;
            reorderBlocks(draggedId, targetId);
            dragIdRef.current = null;
            setDragId(null);
            setOverId(null);
          }}
        >
          {blocks.length === 0 && (
            <div className="text-xs text-text-secondary opacity-60 italic py-4 text-center">
              {t(lang, 'cmdBlocksEmpty')}
            </div>
          )}
          {blocks.map((block, idx) => {
            const cfg = BLOCK_TYPE_CONFIG[block.type];
            const isExpanded = expandedIds.has(block.id);
            const isDragging = dragId === block.id;
            const isOver = overId === block.id;
            const blockBytes = computed.perBlock[idx]?.[0];
            return (
              <div
                key={block.id}
                data-block-id={block.id}
                className={`border rounded-sm transition-all ${cfg.blockClass} ${isDragging ? 'opacity-40' : ''} ${isOver ? 'border-t-2 border-t-blue' : ''}`}
                onDragOver={handleDragOver(block.id)}
                onDrop={handleDrop(block.id)}
              >
                {/* 块头: 点击展开/折叠, 拖拽手柄单独 draggable */}
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
                  {blockBytes && (
                    <span className="text-[9px] text-text-secondary font-mono opacity-70 flex-shrink-0">
                      [{blockBytes.length}B]
                    </span>
                  )}
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
                    {/* 通用: label */}
                    <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
                      <label className="text-[10px] text-text-secondary">{t(lang, 'cmdBlockLabel')}</label>
                      <input
                        type="text"
                        value={block.label ?? ''}
                        onChange={(e) => updateBlock(block.id, { label: e.target.value })}
                        className="w-full px-1.5 py-0.5 bg-bg-input text-text-primary border border-border rounded-sm text-xs focus:outline-none focus:border-accent"
                        placeholder={t(lang, 'cmdBlockLabelPlaceholder')}
                      />
                    </div>

                    {/* const_hex */}
                    {block.type === 'const_hex' && (
                      <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
                        <label className="text-[10px] text-text-secondary">HEX</label>
                        <input
                          type="text"
                          value={block.hex ?? ''}
                          onChange={(e) => updateBlock(block.id, { hex: e.target.value })}
                          className="w-full px-1.5 py-0.5 bg-bg-input text-text-primary border border-border rounded-sm text-xs font-mono focus:outline-none focus:border-accent"
                          placeholder="AA 01 02"
                          spellCheck={false}
                        />
                      </div>
                    )}

                    {/* var_ref */}
                    {block.type === 'var_ref' && (
                      <>
                        <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
                          <label className="text-[10px] text-text-secondary">{t(lang, 'cmdBlockPortName')}</label>
                          <input
                            type="text"
                            value={block.portName ?? ''}
                            onChange={(e) => updateBlock(block.id, { portName: e.target.value })}
                            className="w-full px-1.5 py-0.5 bg-bg-input text-text-primary border border-border rounded-sm text-xs font-mono focus:outline-none focus:border-accent"
                            placeholder="speed"
                            spellCheck={false}
                          />
                        </div>
                        <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
                          <label className="text-[10px] text-text-secondary">{t(lang, 'cmdBlockType')}</label>
                          <select
                            value={block.fieldType ?? 'uint16LE'}
                            onChange={(e) => updateBlock(block.id, { fieldType: e.target.value as FieldType })}
                            className="w-full px-1.5 py-0.5 bg-bg-input text-text-primary border border-border rounded-sm text-xs focus:outline-none focus:border-accent"
                          >
                            {FIELD_TYPE_OPTIONS.map((ft) => (
                              <option key={ft} value={ft}>{ft}</option>
                            ))}
                          </select>
                        </div>
                        <div className="text-[10px] text-text-secondary opacity-70 px-1">
                          {t(lang, 'cmdBlockVarRefHint')}: {String(graphInputs[block.portName ?? 'value'] ?? 0)}
                        </div>
                      </>
                    )}

                    {/* typed_const */}
                    {block.type === 'typed_const' && (
                      <>
                        <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
                          <label className="text-[10px] text-text-secondary">{t(lang, 'cmdBlockType')}</label>
                          <select
                            value={block.fieldType ?? 'uint8'}
                            onChange={(e) => updateBlock(block.id, { fieldType: e.target.value as FieldType })}
                            className="w-full px-1.5 py-0.5 bg-bg-input text-text-primary border border-border rounded-sm text-xs focus:outline-none focus:border-accent"
                          >
                            {FIELD_TYPE_OPTIONS.map((ft) => (
                              <option key={ft} value={ft}>{ft}</option>
                            ))}
                          </select>
                        </div>
                        <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
                          <label className="text-[10px] text-text-secondary">{t(lang, 'cmdBlockValue')}</label>
                          <input
                            type="text"
                            value={block.value ?? ''}
                            onChange={(e) => updateBlock(block.id, { value: e.target.value })}
                            className="w-full px-1.5 py-0.5 bg-bg-input text-text-primary border border-border rounded-sm text-xs font-mono focus:outline-none focus:border-accent"
                            placeholder="0"
                            spellCheck={false}
                          />
                        </div>
                      </>
                    )}

                    {/* checksum */}
                    {block.type === 'checksum' && (
                      <>
                        <div className="grid grid-cols-[60px_1fr] gap-1.5 items-center">
                          <label className="text-[10px] text-text-secondary">{t(lang, 'cmdChecksum')}</label>
                          <select
                            value={block.checksum ?? 'sum8'}
                            onChange={(e) => updateBlock(block.id, { checksum: e.target.value as ChecksumType })}
                            className="w-full px-1.5 py-0.5 bg-bg-input text-text-primary border border-border rounded-sm text-xs focus:outline-none focus:border-accent"
                          >
                            {CHECKSUM_OPTIONS.map((opt) => (
                              <option key={opt.value} value={opt.value}>
                                {t(lang, opt.labelKey)}
                              </option>
                            ))}
                          </select>
                        </div>
                        {block.checksum === 'custom' && (
                          <div className="flex flex-col gap-1">
                            <label className="text-[10px] text-text-secondary">{t(lang, 'cmdCustomScript')}</label>
                            <textarea
                              className="w-full font-mono text-xs bg-bg-input text-text-primary border border-border rounded-sm px-1.5 py-1 outline-none focus:border-accent resize-y min-h-[60px] leading-relaxed"
                              value={block.customScript ?? ''}
                              onChange={(e) => updateBlock(block.id, { customScript: e.target.value })}
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
                        <div className="text-[10px] text-text-secondary opacity-70 px-1">
                          {t(lang, 'cmdBlockChecksumHint')}
                        </div>
                      </>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>

        {/* 添加块按钮 */}
        <div className="flex flex-wrap gap-1 pt-1 border-t border-border flex-shrink-0">
          {(Object.keys(BLOCK_TYPE_CONFIG) as BlockType[]).map((bt) => {
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

      {/* 侧栏: 预览 + 发送 + 全局设置 (固定宽, 纵向滚动) */}
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
          onClick={handleSend}
          disabled={!computed.bytes || computed.bytes.length === 0 || !!computed.error}
        >
          <Send size={12} />
          <span>{t(lang, 'cmdSend')}</span>
        </button>

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

        {/* 全局设置 (始终展开) */}
        <div className="text-[10px] text-text-secondary uppercase tracking-wide font-semibold pt-1">{t(lang, 'cmdSettings')}</div>
        <div className="flex flex-col gap-2 p-2 bg-bg-editor border border-border rounded">
          <div className="grid grid-cols-[80px_1fr] items-center gap-2">
            <label className="text-xs text-text-secondary">{t(lang, 'cmdLabel')}</label>
            <input
              type="text"
              value={params.label}
              onChange={(e) => updateParams({ label: e.target.value })}
              className="text-xs w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded focus:outline-none focus:border-accent transition-colors"
            />
          </div>
          <div className="grid grid-cols-[80px_1fr] items-center gap-2">
            <label className="text-xs text-text-secondary">{t(lang, 'cmdAppendNewline')}</label>
            <button
              className={`bg-bg-input border border-border text-text-secondary px-2 py-0.5 text-xs rounded-sm cursor-pointer transition-all hover:text-text-primary ${params.appendNewline ? 'bg-bg-button text-text-inverse border-bg-button' : ''}`}
              onClick={() => updateParams({ appendNewline: !params.appendNewline })}
            >
              {params.appendNewline ? t(lang, 'cmdNewlineOn') : t(lang, 'cmdNewlineOff')}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
