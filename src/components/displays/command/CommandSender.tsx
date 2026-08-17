import { useState, useMemo, useRef, useCallback, useEffect } from 'react';
import type { WidgetConfig, BlockType, CommandBlock } from '../../../types';
import { useAppStore } from '../../../store/appStore';
import { api } from '../../../lib/tauri/tauri';
import { useGraphInputs } from '../../../lib/hooks/useGraphInput';
import { computeChecksum, type ChecksumKind } from '../../../lib/utils/checksum';
import { parseHex, packField, bytesToHex } from '../../../lib/utils/commandParser';
import { t } from '../../../i18n';
import { nanoid } from 'nanoid';
import { concatChunks } from './commandSenderShared';
import { CommandSenderBlockList } from './CommandSenderBlockList';
import { CommandSenderSidebar } from './CommandSenderSidebar';

interface CommandSenderProps {
  widget: Extract<WidgetConfig, { kind: 'Command' }>;
  onRemove: () => void;
}

/// 命令发送控件 — 数据块拼接方式
export function CommandSender({ widget }: CommandSenderProps) {
  const params = widget.params;
  const { id, blocks } = params;
  const updateWidget = useAppStore((s) => s.updateWidget);
  const sendData = useAppStore((s) => s.sendData);
  const sendAndCapture = useAppStore((s) => s.sendAndCapture);
  const lang = useAppStore((s) => s.lang);

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
  const dragIdRef = useRef<string | null>(null);

  const toggleExpand = (blockId: string) => {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(blockId)) next.delete(blockId);
      else next.add(blockId);
      return next;
    });
  };

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

  const doSend = useCallback(async (): Promise<boolean> => {
    if (!computed.bytes || computed.bytes.length === 0 || computed.error) return false;
    try {
      if (params.loopbackEnabled) {
        const bytes = Array.from(computed.bytes);
        await sendAndCapture(bytes);
        await api.injectLoopbackBytes(id, bytes);
      } else {
        await sendData(Array.from(computed.bytes));
      }
      sendCountRef.current += 1;
      setLastSent(`${new Date().toLocaleTimeString()} #${sendCountRef.current} [${computed.bytes.length}B] ${bytesToHex(computed.bytes)}`);
      return true;
    } catch (e) {
      setError((e as Error).message);
      return false;
    }
  }, [computed, params.loopbackEnabled, sendAndCapture, sendData, id]);

  const doSendRef = useRef(doSend);
  useEffect(() => { doSendRef.current = doSend; }, [doSend]);

  const sendMode = params.sendMode ?? 'manual';
  const timerMs = params.timerMs ?? 100;

  useEffect(() => {
    if (sendMode !== 'timer') return;
    const id = setInterval(() => { void doSendRef.current(); }, timerMs);
    return () => clearInterval(id);
  }, [sendMode, timerMs]);

  const lastAutoSentHexRef = useRef<string | null>(null);
  useEffect(() => {
    if (sendMode !== 'onChange' || !computed.bytes || computed.error) {
      lastAutoSentHexRef.current = null;
      return;
    }
    const hex = bytesToHex(computed.bytes);
    if (lastAutoSentHexRef.current === null) {
      lastAutoSentHexRef.current = hex;
      return;
    }
    if (hex !== lastAutoSentHexRef.current) {
      lastAutoSentHexRef.current = hex;
      void doSendRef.current();
    }
  }, [sendMode, computed]);

  const handleSend = async () => {
    setError(null);
    if (!computed.bytes || computed.bytes.length === 0) {
      setError(t(lang, 'cmdErrorEmpty'));
      return;
    }
    await doSend();
  };

  const updateParams = (changes: Partial<typeof params>) => {
    updateWidget(id, { kind: 'Command', params: { ...params, ...changes } });
  };

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

  const updateBlock = (blockId: string, changes: Partial<CommandBlock>) => {
    updateParams({
      blocks: blocks.map((b) => (b.id === blockId ? { ...b, ...changes } : b)),
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

  return (
    <div className="bg-bg-sidebar border border-border rounded flex-1 min-w-0 min-h-0 flex relative overflow-hidden">
      <CommandSenderBlockList
        blocks={blocks}
        expandedIds={expandedIds}
        dragId={dragId}
        overId={overId}
        computed={computed}
        graphInputs={graphInputs}
        onToggleExpand={toggleExpand}
        onDragStart={handleDragStart}
        onDragOver={handleDragOver}
        onDrop={handleDrop}
        onDragEnd={handleDragEnd}
        onRemoveBlock={removeBlock}
        onUpdateBlock={updateBlock}
        onAddBlock={addBlock}
        onReorderBlocks={reorderBlocks}
        lang={lang}
      />
      <CommandSenderSidebar
        params={params}
        computed={computed}
        error={error}
        lastSent={lastSent}
        onSend={handleSend}
        onUpdateParams={updateParams}
        lang={lang}
      />
    </div>
  );
}
