import { useState, useMemo, useEffect } from 'react';
import type { WidgetConfig, BlockType, CommandBlock, CommandConfig, CommandFrame, SendTaskRegistration } from '../../../types';
import { useAppStore } from '../../../store/appStore';
import { api } from '../../../lib/tauri/tauri';
import { useNumericInputs } from '../../../lib/hooks/useNumericPort';
import { downstreamProtocolOf } from '../../../store/appStoreHelpers';
import { bytesToHex } from '../../../lib/utils/commandParser';
import {
  normalizeCommandConfig,
  commandInputPortNames,
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
}

/// 预览字节刷新防抖 (ms) — 输入变化到预览更新的 IPC 间隔
const PREVIEW_DEBOUNCE_MS = 120;
/// 自动发送任务注册防抖 (ms) — 帧编辑连发合并
const TASK_SYNC_DEBOUNCE_MS = 200;

const EMPTY_COMPUTED: ComputedFrame = { bytes: null, error: null, perBlock: [] };

function toComputedFrame(dto: { bytes: number[] | null; error: string | null; per_block: number[][] }): ComputedFrame {
  return {
    bytes: dto.bytes ? new Uint8Array(dto.bytes) : null,
    error: dto.error,
    // 每块的权威字节 (后端逐块返回 1 段; UI 按块渲染)
    perBlock: dto.per_block.map((chunk) => [new Uint8Array(chunk)]),
  };
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
///
/// 三路发送共用一个 Rust 内核 (schema_engine::compute_frame_bytes + 字节路由):
/// - 预览: 防抖拉取后端权威字节 (本组件不做字节计算)
/// - 手动: send_command_frame (运行态门控 + 同一编码 + 同一路由)
/// - 自动: 非 manual 帧注册到后端调度器 (send_scheduler_ticker), 前端无定时器
export function CommandSender({ widget }: CommandSenderProps) {
  const params = widget.params;
  const { id } = params;
  const updateWidget = useAppStore((s) => s.updateWidget);
  const sendAndCapture = useAppStore((s) => s.sendAndCapture);
  const rfEdges = useAppStore((s) => s.rfEdges);
  const lang = useAppStore((s) => s.lang);
  const runState = useAppStore((s) => s.runState);
  const canSend = runState === 'running';

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

  // 当前帧权威字节 (预览) — 与发送同一 Rust 内核, 防抖合并连续编辑
  const [computed, setComputed] = useState<ComputedFrame>(EMPTY_COMPUTED);
  const graphInputsKey = JSON.stringify(graphInputs);
  useEffect(() => {
    const timer = setTimeout(() => {
      void api.computeFrameBytes(activeFrame, JSON.parse(graphInputsKey) as Record<string, number>)
        .then((res) => { setComputed(toComputedFrame(res)); })
        .catch(() => { /* 纯浏览器 dev / IPC 瞬断: 保留上次预览 */ });
    }, PREVIEW_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [activeFrame, graphInputsKey]);

  // 自动发送任务注册 — 非 manual 帧全量同步到后端调度器。
  // 发送任务是工作区图的后台作业, 不随面板卸载注销 (切页/关闭监视器不影响
  // 已启动的数据链); widget 节点删除后由调度器周期清理兜底。
  const autoTasks = useMemo<SendTaskRegistration[]>(
    () => frames
      .filter((f) => f.sendMode !== 'manual')
      .map((f) => ({
        widgetId: id,
        frameId: f.id,
        mode: f.sendMode === 'timer' ? 'timer' : 'onChange',
        intervalMs: f.timerMs,
        frame: f,
      })),
    [frames, id]
  );
  const autoTasksKey = JSON.stringify(autoTasks);
  useEffect(() => {
    const tasks = JSON.parse(autoTasksKey) as typeof autoTasks;
    const timer = setTimeout(() => {
      void api.setWidgetSendTasks(id, tasks).catch((e) => {
        console.warn('自动发送任务注册失败:', e);
      });
    }, TASK_SYNC_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [id, autoTasksKey]);

  const toggleExpand = (blockId: string) => {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(blockId)) next.delete(blockId);
      else next.add(blockId);
      return next;
    });
  };

  // 手动发送 — 统一内核: 运行态门控 + 后端编码 + 字节边路由, 失败直接返回
  const handleSend = async () => {
    setError(null);
    if (!canSend) {
      setError(t(lang, 'cmdSendRequiresRun'));
      return;
    }
    try {
      const outcome = await api.sendCommandFrame(id, activeFrame, graphInputs);
      setComputed(toComputedFrame(outcome.computed));
      if (outcome.error) {
        setError(outcome.error);
        return;
      }
      if (!outcome.sent) return;
      const bytes = outcome.computed.bytes ?? [];
      if (params.loopbackEnabled) {
        // 回环历史: 用第一个 Transport + 其下游 Protocol 做即时解析对照 (尽力而为)
        const st = useAppStore.getState();
        const transport = st.rfNodes.find((n) => n.type === 'transport' && n.data?.global === true);
        const protocolId = transport
          ? downstreamProtocolOf(transport.id, st.rfEdges, st.rfNodes) ??
            st.rfNodes.find((n) => n.type === 'protocol' && n.data?.global === true)?.id
          : undefined;
        if (transport && protocolId) {
          await sendAndCapture(transport.id, protocolId, bytes);
        }
      }
      setLastSent(`${new Date().toLocaleTimeString()} [${activeFrame.label}] [${bytes.length}B] ${bytesToHex(new Uint8Array(bytes))}`);
    } catch (e) {
      setError((e as Error).message);
    }
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
    setDragId(blockId);
  };

  const handleDragOver = (blockId: string) => (e: React.DragEvent) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
    if (dragId && dragId !== blockId) setOverId(blockId);
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
    const draggedId = e.dataTransfer.getData('text/plain') || dragId;
    if (!draggedId) return;
    reorderBlocks(draggedId, targetId);
    setDragId(null);
    setOverId(null);
  };

  const handleDragEnd = () => {
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
          notRunning={!canSend}
          onSend={() => { void handleSend(); }}
          onUpdateParams={updateParams}
          onUpdateFrame={(changes) => updateFrame(activeFrame.id, changes)}
          lang={lang}
        />
      </div>
    </div>
  );
}
