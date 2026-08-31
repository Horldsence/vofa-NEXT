import { useState, useMemo, useRef, useCallback, useEffect } from 'react';
import type { WidgetConfig, BlockType, CommandBlock, CommandConfig, CommandFrame } from '../../../types';
import { useAppStore } from '../../../store/appStore';
import { api } from '../../../lib/tauri/tauri';
import { useNumericInputs } from '../../../lib/hooks/useNumericPort';
import { downstreamProtocolOf } from '../../../store/appStoreHelpers';
import { bytesToHex } from '../../../lib/utils/commandParser';
import {
  normalizeCommandConfig,
  commandInputPortNames,
  computeFrameBytes,
  makeEmptyFrame,
  type ComputedFrame,
} from '../../../lib/utils/commandFrames';
import { t, type Lang } from '../../../i18n';
import { activateOnKeyboard } from '../../../lib/utils/a11y';
import { nanoid } from 'nanoid';
import { Plus, X } from 'lucide-react';
import { CommandSenderBlockList } from './CommandSenderBlockList';
import { CommandSenderSidebar } from './CommandSenderSidebar';

interface CommandSenderProps {
  widget: Extract<WidgetConfig, { kind: 'Command' }>;
  onRemove: () => void;
}

type SendFrameFn = (frame: CommandFrame, bytes: Uint8Array) => Promise<boolean>;

/// 单帧自动发送器 — timer / onChange 触发, 每帧独立运行 (不可见组件)
function CommandFrameAutoSend({
  frame,
  graphInputs,
  sendRef,
}: {
  frame: CommandFrame;
  graphInputs: Record<string, number>;
  sendRef: React.RefObject<SendFrameFn>;
}) {
  // 自动发送路径走后端 IPC — 后端单一权威 (cmd_buffer::compute_frame_bytes)。
  // 本地 `computeFrameBytes` 仅作 UI 预览用途, 不参与发送控制流。
  const computed = useMemo(() => computeFrameBytes(frame, graphInputs), [frame, graphInputs]);
  const bytesRef = useRef<Uint8Array | null>(null);
  // 重算后向后端拉取一次权威字节; 失败保留旧 bytes (沿用现有策略)
  useEffect(() => {
    let cancelled = false;
    void api.computeFrameBytes(frame, graphInputs).then((res) => {
      if (cancelled) return;
      if (res.bytes) bytesRef.current = new Uint8Array(res.bytes);
    });
    return () => { cancelled = true; };
    // 触发: graphInputs 或 frame 变化时重拉
  }, [frame, graphInputs]);

  // 定时发送
  useEffect(() => {
    if (frame.sendMode !== 'timer') return;
    const id = setInterval(() => {
      const bytes = bytesRef.current;
      if (bytes && bytes.length > 0) void sendRef.current(frame, bytes);
    }, frame.timerMs);
    return () => clearInterval(id);
  }, [frame, sendRef]);

  // 字节流变化时发送 (首次仅记录, 不立即发送)
  const lastAutoSentHexRef = useRef<string | null>(null);
  useEffect(() => {
    if (frame.sendMode !== 'onChange' || !computed.bytes || computed.error) {
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
      void sendRef.current(frame, computed.bytes);
    }
  }, [frame, computed, sendRef]);

  return null;
}

/// 帧列表条 — tab 切换 + 新增/删除/双击改名
function CommandFrameTabBar({
  frames,
  activeId,
  lang,
  onSelect,
  onAdd,
  onRemove,
  onRename,
}: {
  frames: CommandFrame[];
  activeId: string;
  lang: Lang;
  onSelect: (frameId: string) => void;
  onAdd: () => void;
  onRemove: (frameId: string) => void;
  onRename: (frameId: string, label: string) => void;
}) {
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editingLabel, setEditingLabel] = useState('');

  const commitRename = () => {
    if (editingId && editingLabel.trim()) onRename(editingId, editingLabel.trim());
    setEditingId(null);
  };

  return (
    <div className="flex items-center gap-1 px-2 py-1 border-b border-border shrink-0 overflow-x-auto">
      <span className="text-[10px] text-text-secondary uppercase tracking-wide font-semibold pr-1 shrink-0">
        {t(lang, 'cmdFrames')}
      </span>
      {frames.map((f) => {
        const active = f.id === activeId;
        return (
          <div
            key={f.id}
            className={`inline-flex items-center gap-0.5 px-2 py-0.5 rounded-sm text-[11px] cursor-pointer select-none border transition-colors shrink-0 ${
              active
                ? 'bg-bg-button text-text-inverse border-bg-button'
                : 'bg-bg-input text-text-secondary border-border hover:text-text-primary'
            }`}
            onClick={() => onSelect(f.id)}
            onKeyDown={activateOnKeyboard}
            role="button"
            tabIndex={0}
            onDoubleClick={() => {
              setEditingId(f.id);
              setEditingLabel(f.label);
            }}
            title={t(lang, 'cmdRenameFrameHint')}
          >
            {editingId === f.id ? (
              <input
                type="text"
                value={editingLabel}
                onChange={(e) => setEditingLabel(e.target.value)}
                onBlur={commitRename}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') commitRename();
                  if (e.key === 'Escape') setEditingId(null);
                }}
                onClick={(e) => e.stopPropagation()}
                className="text-[11px] w-20 px-1 py-0 bg-bg-input text-text-primary border border-accent rounded-sm focus:outline-none"
              />
            ) : (
              <span className="whitespace-nowrap">{f.label}</span>
            )}
            {frames.length > 1 && (
              <button
                className="p-0.5 opacity-60 hover:opacity-100 hover:text-red shrink-0"
                title={t(lang, 'cmdRemoveFrame')}
                onClick={(e) => {
                  e.stopPropagation();
                  onRemove(f.id);
                }}
              >
                <X size={10} />
              </button>
            )}
          </div>
        );
      })}
      <button
        className="inline-flex items-center justify-center p-1 rounded-sm border border-dashed border-border text-text-secondary hover:text-text-primary hover:border-accent transition-colors shrink-0"
        title={t(lang, 'cmdAddFrame')}
        onClick={onAdd}
      >
        <Plus size={11} />
      </button>
    </div>
  );
}

/// 命令发送控件 — 多帧, 每帧独立的数据块拼接 / 触发方式
export function CommandSender({ widget }: CommandSenderProps) {
  const params = widget.params;
  const { id } = params;
  const updateWidget = useAppStore((s) => s.updateWidget);
  const sendAndCapture = useAppStore((s) => s.sendAndCapture);
  const rfEdges = useAppStore((s) => s.rfEdges);
  const lang = useAppStore((s) => s.lang);

  // 归一化后的配置 (旧版单帧配置现场包装, updateWidget 落盘时已归一化)
  const config = useMemo<CommandConfig>(() => normalizeCommandConfig(params), [params]);
  const frames = config.frames;

  // 当前选中帧 (activeFrameId 失效时回退第一帧)
  const [activeFrameId, setActiveFrameId] = useState<string | null>(null);
  const activeFrame = frames.find((f) => f.id === activeFrameId) ?? frames[0];
  const blocks = activeFrame.blocks;

  // 字节路由: loopbackOut 出口的字节边 (→ Transport.tx 真实发送 / FrameDecoder.in 喂入 / Protocol.in)
  const hasByteRoute = useMemo(
    () => rfEdges.some((e) => e.source === id && e.sourceHandle === 'loopbackOut'),
    [rfEdges, id]
  );

  // 输入端口 = 所有帧 var_ref 块并集 (与节点 Handle 派生一致)
  const portNames = useMemo(() => commandInputPortNames(params), [params]);
  const inputStates = useNumericInputs(id, portNames);
  const graphInputs = useMemo(
    () => Object.fromEntries(
      portNames.map((port) => [port, inputStates[port]?.latest?.value ?? 0]),
    ),
    [portNames, inputStates],
  );

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

  // 当前帧字节拼接 (预览 / 手动发送)
  const computed = useMemo<ComputedFrame>(
    () => computeFrameBytes(activeFrame, graphInputs),
    [activeFrame, graphInputs]
  );

  // 发送指定帧的字节 (手动 / timer / onChange 共用; 回环为发送器级)
  const sendFrame = useCallback<SendFrameFn>(async (frame, bytes) => {
    if (bytes.length === 0) return false;
    if (!hasByteRoute) {
      setError(t(lang, 'cmdNoByteRoute'));
      return false;
    }
    try {
      // 走前端预计算的字节 (预览由本地 computeFrameBytes 计算) — 自动发送路径由
      // CommandFrameAutoSend 后台拉取后端权威字节, 手动发送沿用预览字节 (已通过
      // handleSend 路径内的后端校验, 见 onChange effect)。
      const arr = Array.from(bytes);
      // 沿字节边图路由注入 (含 Transport.tx 真实发送)
      await api.injectBytes(id, arr);
      if (params.loopbackEnabled) {
        // 回环历史: 用第一个 Transport + 其下游 Protocol 做即时解析对照 (尽力而为)
        const st = useAppStore.getState();
        const transport = st.rfNodes.find((n) => n.type === 'transport' && n.data?.global === true);
        const protocolId = transport
          ? downstreamProtocolOf(transport.id, st.rfEdges, st.rfNodes) ??
            st.rfNodes.find((n) => n.type === 'protocol' && n.data?.global === true)?.id
          : undefined;
        if (transport && protocolId) {
          await sendAndCapture(transport.id, protocolId, arr);
        }
      }
      sendCountRef.current += 1;
      setLastSent(`${new Date().toLocaleTimeString()} #${sendCountRef.current} [${frame.label}] [${bytes.length}B] ${bytesToHex(bytes)}`);
      return true;
    } catch (e) {
      setError((e as Error).message);
      return false;
    }
  }, [params.loopbackEnabled, sendAndCapture, hasByteRoute, id, lang]);

  const sendFrameRef = useRef<SendFrameFn>(sendFrame);
  useEffect(() => { sendFrameRef.current = sendFrame; }, [sendFrame]);

  const handleSend = async () => {
    setError(null);
    // 手动发送走完后端 IPC 拿到权威字节 — 与自动发送路径同源 (避免双计算分歧)
    const res = await api.computeFrameBytes(activeFrame, graphInputs);
    if (!res.bytes || res.bytes.length === 0 || res.error) {
      setError(res.error ?? t(lang, 'cmdErrorEmpty'));
      return;
    }
    await sendFrame(activeFrame, new Uint8Array(res.bytes));
  };

  const updateParams = (changes: Partial<CommandConfig>) => {
    updateWidget(id, { kind: 'Command', params: { ...config, ...changes } });
  };

  // 帧列表整体替换 (增删/块编辑共用出口)
  const applyFrames = (nextFrames: CommandFrame[]) => updateParams({ frames: nextFrames });

  const updateFrame = (frameId: string, changes: Partial<CommandFrame>) => {
    applyFrames(frames.map((f) => (f.id === frameId ? { ...f, ...changes } : f)));
  };

  const addFrame = () => {
    const frame = makeEmptyFrame(id, `${t(lang, 'cmdNewFrame')} ${frames.length + 1}`);
    applyFrames([...frames, frame]);
    setActiveFrameId(frame.id);
  };

  const removeFrame = (frameId: string) => {
    if (frames.length <= 1) return; // 至少保留一帧
    applyFrames(frames.filter((f) => f.id !== frameId));
  };

  // 当前帧块列表更新
  const applyBlocks = (nextBlocks: CommandBlock[]) => updateFrame(activeFrame.id, { blocks: nextBlocks });

  const addBlock = (type: BlockType) => {
    const defaults: Record<BlockType, Partial<CommandBlock>> = {
      const_hex: { label: '', hex: '00' },
      var_ref: { label: '', portName: `in${portNames.length + 1}`, fieldType: 'uint16LE' },
      typed_const: { label: '', fieldType: 'uint8', value: '0' },
      checksum: { label: '', checksum: 'sum8' },
    };
    const newBlock: CommandBlock = { id: nanoid(6), type, ...defaults[type] };
    applyBlocks([...blocks, newBlock]);
    setExpandedIds((prev) => new Set(prev).add(newBlock.id));
  };

  const updateBlock = (blockId: string, changes: Partial<CommandBlock>) => {
    applyBlocks(blocks.map((b) => (b.id === blockId ? { ...b, ...changes } : b)));
  };

  const removeBlock = (blockId: string) => {
    applyBlocks(blocks.filter((b) => b.id !== blockId));
    setExpandedIds((prev) => {
      const next = new Set(prev);
      next.delete(blockId);
      return next;
    });
  };

  const handleDragStart = (blockId: string) => (e: React.DragEvent) => {
    e.dataTransfer.effectAllowed = 'move';
    e.dataTransfer.setData('text/plain', blockId);
    const blockEl = (e.currentTarget as HTMLElement).closest('[data-block-id]');
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
    applyBlocks(next);
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
    <div className="bg-bg-sidebar border border-border rounded flex-1 min-w-0 min-h-0 flex flex-col relative overflow-hidden">
      <CommandFrameTabBar
        frames={frames}
        activeId={activeFrame.id}
        lang={lang}
        onSelect={setActiveFrameId}
        onAdd={addFrame}
        onRemove={removeFrame}
        onRename={(frameId, label) => updateFrame(frameId, { label })}
      />
      <div className="flex-1 min-w-0 min-h-0 flex relative overflow-hidden">
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
          params={config}
          frame={activeFrame}
          computed={computed}
          error={error}
          lastSent={lastSent}
          routeMissing={!hasByteRoute}
          onSend={() => { void handleSend(); }}
          onUpdateParams={updateParams}
          onUpdateFrame={(changes) => updateFrame(activeFrame.id, changes)}
          lang={lang}
        />
      </div>
      {/* 每帧独立的自动发送器 (timer/onChange) */}
      {frames.map((f) =>
        f.sendMode === 'manual' ? null : (
          <CommandFrameAutoSend key={f.id} frame={f} graphInputs={graphInputs} sendRef={sendFrameRef} />
        )
      )}
    </div>
  );
}
