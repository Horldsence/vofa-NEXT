import { Plus, Trash2, GripVertical, ChevronDown, ChevronRight } from 'lucide-react';
import type { BlockType, CommandBlock } from '../../../types';
import type { Lang } from '../../../i18n';
import { t } from '../../../i18n';
import { BLOCK_TYPE_CONFIG, blockSummary } from './commandSenderShared';
import { CommandBlockEditor } from './CommandSenderBlockEditor';
import { activateOnKeyboard } from '../../../lib/utils/a11y';

interface Props {
  blocks: CommandBlock[];
  expandedIds: Set<string>;
  dragId: string | null;
  overId: string | null;
  computed: {
    bytes: Uint8Array | null;
    error: string | null;
    perBlock: Uint8Array[][];
  };
  graphInputs: Record<string, number>;
  onToggleExpand: (blockId: string) => void;
  onDragStart: (blockId: string) => (e: React.DragEvent) => void;
  onDragOver: (blockId: string) => (e: React.DragEvent) => void;
  onDrop: (targetId: string) => (e: React.DragEvent) => void;
  onDragEnd: () => void;
  onRemoveBlock: (blockId: string) => void;
  onUpdateBlock: (blockId: string, changes: Partial<CommandBlock>) => void;
  onAddBlock: (type: BlockType) => void;
  onReorderBlocks: (fromId: string, toId: string) => void;
  lang: Lang;
}

export function CommandSenderBlockList({
  blocks,
  expandedIds,
  dragId,
  overId,
  computed,
  graphInputs,
  onToggleExpand,
  onDragStart,
  onDragOver,
  onDrop,
  onDragEnd,
  onRemoveBlock,
  onUpdateBlock,
  onAddBlock,
  onReorderBlocks,
  lang,
}: Props) {
  return (
    <div className="flex-1 min-w-0 min-h-0 flex flex-col gap-2 p-3 overflow-y-auto bg-bg-sidebar">
      <div className="flex items-center justify-between pb-1.5 border-b border-border shrink-0">
        <span className="text-base font-semibold text-text-bright">Command Sender</span>
        <span className="text-[10px] text-text-secondary">{blocks.length} blocks</span>
      </div>

      {/* 块列表 */}
      <div
        className="flex flex-col gap-1.5"
        onDragOver={(e) => {
          if (dragId) e.preventDefault();
          e.dataTransfer.dropEffect = 'move';
        }}
        onDrop={(e) => {
          e.preventDefault();
          const draggedId = e.dataTransfer.getData('text/plain') || dragId;
          const targetId = overId;
          if (!draggedId || !targetId) return;
          onReorderBlocks(draggedId, targetId);
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
              onDragOver={onDragOver(block.id)}
              onDrop={onDrop(block.id)}
            >
              {/* 块头 */}
              <div
                className="flex items-center gap-1.5 px-1.5 py-1 cursor-pointer select-none"
                onClick={() => onToggleExpand(block.id)}
                onKeyDown={activateOnKeyboard}
                role="button"
                tabIndex={0}
              >
                <div
                  className="inline-flex items-center justify-center p-0.5 cursor-grab active:cursor-grabbing text-text-secondary hover:text-text-primary shrink-0"
                  title={t(lang, 'cmdDragToReorder')}
                  draggable
                  onDragStart={onDragStart(block.id)}
                  onDragEnd={onDragEnd}
                >
                  <GripVertical size={12} className="pointer-events-none" />
                </div>
                <span
                  className={`inline-flex items-center gap-0.5 px-1 py-0.5 rounded-sm text-[9px] font-semibold uppercase tracking-wide shrink-0 border ${cfg.badgeClass}`}
                >
                  {cfg.icon}
                  {t(lang, cfg.labelKey)}
                </span>
                {block.label && (
                  <span className="text-xs text-text-primary truncate shrink-0">{block.label}</span>
                )}
                <span className="text-[10px] text-text-secondary font-mono truncate flex-1 min-w-0">
                  {blockSummary(block)}
                </span>
                {blockBytes && (
                  <span className="text-[9px] text-text-secondary font-mono opacity-70 shrink-0">
                    [{blockBytes.length}B]
                  </span>
                )}
                <span className="text-text-secondary shrink-0 p-0.5 pointer-events-none">
                  {isExpanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
                </span>
                <button
                  className="text-text-secondary hover:text-red shrink-0 p-0.5"
                  onClick={(e) => { e.stopPropagation(); onRemoveBlock(block.id); }}
                  title={t(lang, 'removeWidget')}
                >
                  <Trash2 size={11} />
                </button>
              </div>

              {/* 块编辑区 */}
              {isExpanded && (
                <CommandBlockEditor
                  block={block}
                  updateBlock={onUpdateBlock}
                  lang={lang}
                  graphInputs={graphInputs}
                />
              )}
            </div>
          );
        })}
      </div>

      {/* 添加块按钮 */}
      <div className="flex flex-wrap gap-1 pt-1 border-t border-border shrink-0">
        {(Object.keys(BLOCK_TYPE_CONFIG) as BlockType[]).map((bt) => {
          const cfg = BLOCK_TYPE_CONFIG[bt];
          return (
            <button
              key={bt}
              className="inline-flex items-center gap-1 bg-transparent border border-dashed border-border text-text-secondary px-2 py-1 text-[11px] rounded-sm cursor-pointer transition-all hover:text-text-primary hover:border-accent"
              onClick={() => onAddBlock(bt)}
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
  );
}
