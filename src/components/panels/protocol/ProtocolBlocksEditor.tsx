//! 协议 schema 自定义块编辑器 — preset='custom' 时内嵌于 ProtocolConfigForm
//! 支持协议侧有意义的块: header/length/id/field/bitfield/checksum/tail/csv
//! (块编辑表单复用 FrameDecoder 的 BlockEditor; 本组件只维护列表增删/排序/展开)
import { useState } from 'react';
import { Plus, Trash2, ChevronDown, ChevronRight, ArrowUp, ArrowDown } from 'lucide-react';
import { nanoid } from 'nanoid';
import { t, type Lang } from '../../../i18n';
import type { DecoderBlock } from '../../../types';
import { BLOCK_TYPE_CONFIG, blockSummary } from '../../displays/decoder/frameDecoderShared';
import { BlockEditor } from '../../displays/decoder/FrameDecoderBlockEditor';
import { activateOnKeyboard } from '../../../lib/utils/a11y';

/// 协议 schema 编辑器可添加的块类型
const PROTOCOL_BLOCK_TYPES = [
  'header', 'length', 'id', 'field', 'bitfield', 'checksum', 'tail', 'csv',
] as const;

type ProtocolBlockType = (typeof PROTOCOL_BLOCK_TYPES)[number];

interface ProtocolBlocksEditorProps {
  blocks: DecoderBlock[];
  onChange: (blocks: DecoderBlock[]) => void;
  lang: Lang;
}

export function ProtocolBlocksEditor({ blocks, onChange, lang }: ProtocolBlocksEditorProps) {
  const [expandedIds, setExpandedIds] = useState<Set<number>>(new Set());

  const toggleExpand = (idx: number) => {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(idx)) next.delete(idx);
      else next.add(idx);
      return next;
    });
  };

  const patchBlock = (idx: number, changes: Partial<DecoderBlock>) => {
    onChange(blocks.map((b, i) => (i === idx ? ({ ...b, ...changes } as DecoderBlock) : b)));
  };

  const removeBlock = (idx: number) => {
    onChange(blocks.filter((_, i) => i !== idx));
    setExpandedIds(new Set());
  };

  const moveBlock = (idx: number, dir: -1 | 1) => {
    const to = idx + dir;
    if (to < 0 || to >= blocks.length) return;
    const next = [...blocks];
    [next[idx], next[to]] = [next[to], next[idx]];
    onChange(next);
    setExpandedIds(new Set());
  };

  const addBlock = (type: ProtocolBlockType) => {
    const defaults: Record<ProtocolBlockType, Partial<DecoderBlock>> = {
      header: { hex: 'AA' },
      length: { fieldType: 'uint8', portName: 'length', unit: 'bytes' },
      id: { fieldType: 'uint8', portName: 'id_value' },
      field: { fieldType: 'float32LE', portName: `ch${blocks.length}` },
      bitfield: { byteOffset: 0, bitOffset: 0, bitLength: 4, isSigned: false, portName: `bits_${blocks.length}` },
      checksum: { algorithm: 'sum8', cover: 'all_prior', position: 'append' },
      tail: { hex: '00 00 80 7F' },
      csv: { separator: ',', ports: ['ch0', 'ch1'] },
    };
    // id 仅前端 UI 引用 (Rust 端扩展块无 id, serde 忽略未知字段)
    const newBlock = { id: nanoid(6), type, ...defaults[type] } as DecoderBlock;
    onChange([...blocks, newBlock]);
    setExpandedIds(new Set([blocks.length]));
  };

  return (
    <div className="flex flex-col gap-1.5">
      {blocks.length === 0 && (
        <div className="text-xs text-text-secondary opacity-60 italic py-2 text-center">
          {t(lang, 'fdBlocksEmpty')}
        </div>
      )}
      {blocks.map((block, idx) => {
        const cfg = BLOCK_TYPE_CONFIG[block.type];
        const isExpanded = expandedIds.has(idx);
        return (
          <div key={block.id ?? idx} className={`border rounded-sm ${cfg.blockClass}`}>
            {/* 块头: 类型徽章 + 摘要 + 排序/删除 */}
            <div
              className="flex items-center gap-1 px-1 py-1 cursor-pointer select-none"
              onClick={() => toggleExpand(idx)}
              onKeyDown={activateOnKeyboard}
              role="button"
              tabIndex={0}
            >
              <span className={`inline-flex items-center gap-0.5 px-1 py-0.5 rounded-sm text-[9px] font-semibold uppercase tracking-wide flex-shrink-0 border ${cfg.badgeClass}`}>
                {cfg.icon}
                {t(lang, cfg.labelKey)}
              </span>
              <span className="text-[10px] text-text-secondary font-mono truncate flex-1 min-w-0">
                {blockSummary(block)}
              </span>
              <button
                className="text-text-secondary hover:text-text-primary flex-shrink-0 p-0.5 disabled:opacity-30"
                onClick={(e) => { e.stopPropagation(); moveBlock(idx, -1); }}
                disabled={idx === 0}
                title="↑"
              >
                <ArrowUp size={10} />
              </button>
              <button
                className="text-text-secondary hover:text-text-primary flex-shrink-0 p-0.5 disabled:opacity-30"
                onClick={(e) => { e.stopPropagation(); moveBlock(idx, 1); }}
                disabled={idx === blocks.length - 1}
                title="↓"
              >
                <ArrowDown size={10} />
              </button>
              <span className="text-text-secondary flex-shrink-0 p-0.5 pointer-events-none">
                {isExpanded ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
              </span>
              <button
                className="text-text-secondary hover:text-red flex-shrink-0 p-0.5"
                onClick={(e) => { e.stopPropagation(); removeBlock(idx); }}
                title={t(lang, 'removeWidget')}
              >
                <Trash2 size={10} />
              </button>
            </div>
            {/* 块编辑区 (展开时) — BlockEditor 的 id 参数忽略, 按索引补丁 */}
            {isExpanded && (
              <div className="px-1.5 pb-1.5 flex flex-col gap-1.5">
                <BlockEditor
                  block={block}
                  updateBlock={(_id, changes) => patchBlock(idx, changes)}
                  lang={lang}
                />
              </div>
            )}
          </div>
        );
      })}
      {/* 添加块按钮 */}
      <div className="flex flex-wrap gap-1 pt-1 border-t border-border">
        {PROTOCOL_BLOCK_TYPES.map((bt) => {
          const cfg = BLOCK_TYPE_CONFIG[bt];
          return (
            <button
              key={bt}
              type="button"
              className="inline-flex items-center gap-1 bg-transparent border border-dashed border-border text-text-secondary px-1.5 py-0.5 text-[10px] rounded-sm cursor-pointer transition-all hover:text-text-primary hover:border-accent"
              onClick={() => addBlock(bt)}
              title={t(lang, cfg.addLabelKey)}
            >
              <Plus size={10} />
              <span className={`inline-flex items-center gap-0.5 ${cfg.iconClass}`}>{cfg.icon}</span>
              <span>{t(lang, cfg.addLabelKey)}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
