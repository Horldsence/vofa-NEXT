//! 操作历史 / 撤销重做 — 快照式撤销栈 (会话内有效)
//!
//! 记录模型: entries[0] 为基线快照, 之后每个已记录操作对应一项
//! 「操作完成后的文档快照」; index 指向当前生效的条目。
//! undo/redo/面板任意点跳转统一为「移动 index 并应用该条目快照」,
//! 因此历史面板可以像 Photoshop 一样点击任意一条直接回到那个时刻。
//!
//! 埋点方式: 各 slice 的变更 action 通过 withHistoryOp 包裹
//! (或异步 action 里 beginHistoryOp + commitHistoryOp 成对调用)。
//! 未埋点的写入 (后端事件推送 / 启动种子图) 天然不入栈;
//! 导入备份 / 应用模板走 rebaseHistory 重置为新基线。
//!
//! 快照双轨: data 是恢复用的深拷贝, refs 是捕获时刻各切片的活引用 —
//! 「本次变更是否真的动了文档」用 refs 判定 (克隆体引用永远互不相同);
//! 遗留系统若原地改写嵌套对象, 深拷贝保证 undo 的存档不受后续污染。
//!
//! 恢复副作用: 全量替换切片后必须
//! ① removeDerived 清掉消失全局节点的派生端口表
//! ② closeTransport 关闭消失 Transport 的连接 (尽力而为)
//! ③ syncAllTabGraphs 让后端按恢复后的图重建派生端口与求值引擎。

import { create } from 'zustand';
import type { Node, Edge } from '@xyflow/react';
import { api } from '../lib/tauri/tauri';
import { useAppStore } from './appStore';
import { isGlobalNode } from './appStoreHelpers';
import type { NodeVisualRef } from '../lib/utils/nodeKindVisuals';
import type { WidgetConfig, DataTab, ControlTab } from '../types';

/// 栈深上限 — 超出丢最旧 (基线允许被挤出, 语义仍是「最早可回退点」)
const MAX_ENTRIES = 200;
/// 连续操作合并窗口默认时长 (滑块拖拽 / 节点拖动等高频变更)
export const DEFAULT_COALESCE_MS = 700;

/// 一次操作的展示元信息 (面板列表用)
export interface HistoryOperation {
  /// i18n 主标签键 (opAddWidget 等)
  opKey: string;
  /// i18n 键形式的补充信息 (如控件类型名)
  detailKey?: string;
  /// 原文补充信息 (页签名 / 节点 label 等用户文本, 无需翻译)
  detailText?: string;
  /// 目标节点语义 — 面板据此渲染对应节点的主题色徽章与图标
  target?: HistoryTarget;
}

/** 单节点类操作的目标 (画布上该节点的种类, 决定行首图标/配色) */
export type NodeOpRef = NodeVisualRef;

/**
 * 操作目标的节点语义:
 * - node: 明确归属单个节点 (添加/删除控件、改配置等)
 * - edge: 连线操作 — 双端点各自着色 (from/to 缺失时退化为中性点)
 * - nodes: 泛节点批操作 (拖动 / 混合删除), 无单一归属
 * - tab / doc: 页签级与工作区级操作
 */
export type HistoryTarget =
  | { kind: 'node'; node: NodeOpRef }
  | { kind: 'edge'; from?: NodeOpRef; to?: NodeOpRef }
  | { kind: 'nodes' }
  | { kind: 'tab' }
  | { kind: 'doc' };

export interface TrackOptions {
  /// 相同 key 且落在时间窗内的连续提交替换栈顶而不是新增 (滑块/拖动合并)
  coalesceKey?: string;
  coalesceMs?: number;
}

/// 文档域快照 — 撤销范围内全部 appStore 切片的整存整取
export interface DocSnapshot {
  rfNodes: Node[];
  rfEdges: Edge[];
  widgets: WidgetConfig[];
  controlTabs: ControlTab[];
  activeControlTabId: string;
  dataTabs: DataTab[];
  activeDataTabId: string;
}

/// 快照的活引用对 — 捕获时刻各切片的原始引用。
/// data 经深拷贝与 store 脱钩后引用永不相同, 等值判断必须走这份原始引用。
type DocRefs = DocSnapshot;

/// 单条记录的完整载荷: 恢复用深拷贝 + 等值判断用活引用
interface DocRecord {
  data: DocSnapshot;
  refs: DocRefs;
}

export interface HistoryEntry extends HistoryOperation {
  id: number;
  time: number;
  snapshot: DocSnapshot;
}

/// 入栈的实际类型 — 在公开条目之上多挂一份等值判断用的活引用
interface InternalHistoryEntry extends HistoryEntry {
  refs: DocRefs;
}

interface HistoryState {
  entries: HistoryEntry[];
  /// 当前生效条目下标 (0 = 基线); undo 后 < length-1, 可 redo
  index: number;
  canUndo: boolean;
  canRedo: boolean;
  undo: () => void;
  redo: () => void;
  jumpTo: (id: number) => void;
  clearHistory: () => void;
}

// ---- 非响应式录制簿记 (不入 store, 避免面板订阅抖动) ----

let nextEntryId = 1;
let restoring = false;
let lastCoalesceKey: string | null = null;
let lastCoalesceAt = 0;

function deepClone<T>(value: T): T {
  if (typeof structuredClone === 'function') return structuredClone(value);
  return JSON.parse(JSON.stringify(value)) as T;
}

/** 捕获当前文档态: refs 取活引用, data 为其独立深拷贝 */
function captureDoc(): DocRecord {
  const s = useAppStore.getState();
  const raw: DocSnapshot = {
    rfNodes: s.rfNodes,
    rfEdges: s.rfEdges,
    widgets: s.widgets,
    controlTabs: s.controlTabs,
    activeControlTabId: s.activeControlTabId,
    dataTabs: s.dataTabs,
    activeDataTabId: s.activeDataTabId,
  };
  return { data: deepClone(raw), refs: raw };
}

function snapshotsEqual(a: DocRefs, b: DocRefs): boolean {
  return (
    a.rfNodes === b.rfNodes &&
    a.rfEdges === b.rfEdges &&
    a.widgets === b.widgets &&
    a.controlTabs === b.controlTabs &&
    a.activeControlTabId === b.activeControlTabId &&
    a.dataTabs === b.dataTabs &&
    a.activeDataTabId === b.activeDataTabId
  );
}

function derivedFlags(entries: HistoryEntry[], index: number) {
  return { canUndo: index > 0, canRedo: index < entries.length - 1 };
}

function makeBaseline(opKey = 'opHistoryInitial'): InternalHistoryEntry[] {
  const doc = captureDoc();
  return [
    {
      id: nextEntryId++,
      time: Date.now(),
      opKey,
      snapshot: doc.data,
      refs: doc.refs,
    },
  ];
}

export const useHistoryStore = create<HistoryState>()((set, get) => ({
  entries: [],
  index: 0,
  ...derivedFlags([], 0),

  undo: () => {
    const { entries, index } = get();
    if (index <= 0 || index >= entries.length) return;
    const target = index - 1;
    restoreSnapshot(entries[target].snapshot);
    set({ index: target, ...derivedFlags(entries, target) });
  },

  redo: () => {
    const { entries, index } = get();
    if (index < 0 || index >= entries.length - 1) return;
    const target = index + 1;
    restoreSnapshot(entries[target].snapshot);
    set({ index: target, ...derivedFlags(entries, target) });
  },

  jumpTo: (id) => {
    const { entries, index } = get();
    const target = entries.findIndex((e) => e.id === id);
    if (target < 0 || target === index) return;
    restoreSnapshot(entries[target].snapshot);
    set({ index: target, ...derivedFlags(entries, target) });
  },

  clearHistory: () => {
    const entries = makeBaseline();
    set({ entries, index: 0, ...derivedFlags(entries, 0) });
    lastCoalesceKey = null;
    lastCoalesceAt = 0;
  },
}));

/// 把某个历史状态整体应用回 appStore, 并补齐恢复副作用。
/// restoring 标志保证恢复期间不入栈 (undo 本身不是一次「用户操作」)。
function restoreSnapshot(snapshot: DocSnapshot): void {
  const prev = useAppStore.getState();
  // 消失的全局节点: 清派生端口表; 其中 Transport 还要尽力关闭连接
  const prevGlobalIds = new Set(prev.rfNodes.filter(isGlobalNode).map((n) => n.id));
  const nextGlobalIds = new Set(snapshot.rfNodes.filter(isGlobalNode).map((n) => n.id));
  const removedGlobal = [...prevGlobalIds].filter((id) => !nextGlobalIds.has(id));
  const removedTransports = new Set(
    removedGlobal.filter((id) => prev.rfNodes.find((n) => n.id === id)?.type === 'transport')
  );

  restoring = true;
  try {
    // 独立查看面板 (无 widgetId 的 data tab: 操作历史 / CAN / 逻辑 / 固定编译页)
    // 属于 UI 态而非文档态 — 回滚文档时不关闭它们, 否则用户正看着的历史面板
    // 会在跳回旧快照的瞬间被一并"退没"。把控恢复前仍打开的此类 tab 合并回快照。
    let restoredTabs = snapshot.dataTabs;
    for (const t of prev.dataTabs) {
      if (t.widgetId != null) continue; // 控件派生窗口跟随文档回滚
      if (!restoredTabs.some((x) => x.id === t.id)) {
        restoredTabs = [...restoredTabs, t];
      }
    }
    // 恢复前正看着某个独立面板 → 恢复后保持不动, 便于继续操作
    const stayActive =
      prev.activeDataTabId !== snapshot.activeDataTabId &&
      prev.dataTabs.some((t) => t.id === prev.activeDataTabId && t.widgetId == null);
    const activeDataTabId = stayActive ? prev.activeDataTabId : snapshot.activeDataTabId;

    useAppStore.setState({
      rfNodes: deepClone(snapshot.rfNodes),
      rfEdges: deepClone(snapshot.rfEdges),
      widgets: deepClone(snapshot.widgets),
      controlTabs: deepClone(snapshot.controlTabs),
      activeControlTabId: snapshot.activeControlTabId,
      dataTabs: deepClone(restoredTabs),
      activeDataTabId,
    });
    if (removedGlobal.length > 0) {
      useAppStore.getState().removeDerived(removedGlobal);
    }
    useAppStore.getState().syncAllTabGraphs();
  } finally {
    restoring = false;
  }
  for (const id of removedTransports) {
    void api.closeTransport(id).catch(() => {});
  }
}

// ============================================================
// 录制原语 — 供各 slice 埋点使用
// ============================================================

/// 惰性基线: 首个被记录的操作之前捕获一次初始状态。
/// 启动期的种子图 / 默认初始化发生在首个埋点之前, 因此天然落进基线、不可撤销。
export function beginHistoryOp(): void {
  if (restoring) return;
  if (useHistoryStore.getState().entries.length === 0) {
    const entries = makeBaseline();
    useHistoryStore.setState({ entries, index: 0, ...derivedFlags(entries, 0) });
  }
}

/// 提交一次操作: 变更执行完毕后捕获当前文档态入栈。
/// - 同 key 且在时间窗内的连续提交 → 替换栈顶 (连续手势合并为一条)
/// - 与栈顶活引用完全一致 (无 tracked 切片变化) → 跳过
/// - 撤销后产生的新操作 → 丢弃 redo 分支 (标准行为)
export function commitHistoryOp(op: HistoryOperation, opts?: TrackOptions): void {
  if (restoring) return;
  const st = useHistoryStore.getState();
  if (st.entries.length === 0) return; // 未 begin 就 commit — 视为无效调用
  const now = Date.now();
  const windowMs = opts?.coalesceMs ?? DEFAULT_COALESCE_MS;
  const current = captureDoc();

  const atTop = st.index === st.entries.length - 1;
  const entriesAtTop = st.entries as InternalHistoryEntry[];
  if (
    opts?.coalesceKey &&
    opts.coalesceKey === lastCoalesceKey &&
    now - lastCoalesceAt < windowMs &&
    atTop
  ) {
    // 合并进栈顶: 保留首条操作的元信息 (id/时间/标签), 刷新其快照
    const entries = entriesAtTop.slice();
    entries[entries.length - 1] = {
      ...entries[entries.length - 1],
      snapshot: current.data,
      refs: current.refs,
    };
    useHistoryStore.setState({
      entries,
      index: entries.length - 1,
      ...derivedFlags(entries, entries.length - 1),
    });
  } else if (
    atTop &&
    snapshotsEqual(current.refs, entriesAtTop[entriesAtTop.length - 1].refs)
  ) {
    // 无 tracked 切片变化 (如仅打开编辑器等 UI 副作用) — 不产生历史条目
    return;
  } else {
    // 新操作: 截断 redo 分支 (index 之后的部分), 追加到栈顶
    let entries: InternalHistoryEntry[] = entriesAtTop.slice(0, st.index + 1);
    entries.push({
      id: nextEntryId++,
      time: now,
      opKey: op.opKey,
      detailKey: op.detailKey,
      detailText: op.detailText,
      target: op.target,
      snapshot: current.data,
      refs: current.refs,
    });
    let index = entries.length - 1;
    if (entries.length > MAX_ENTRIES) {
      entries = entries.slice(entries.length - MAX_ENTRIES);
      index = entries.length - 1;
    }
    useHistoryStore.setState({ entries, index, ...derivedFlags(entries, index) });
  }
  lastCoalesceKey = opts?.coalesceKey ?? null;
  lastCoalesceAt = now;
}

/// 包裹一个同步变更 action 体: 进入前惰性建基线, 成功返回后提交。
export function withHistoryOp<T>(op: HistoryOperation, fn: () => T, opts?: TrackOptions): T {
  beginHistoryOp();
  const result = fn();
  commitHistoryOp(op, opts);
  return result;
}

/// 重置历史并以当前状态为新基线 — 导入备份 / 应用模板后调用。
/// baselineOpKey 决定基线条目在历史面板中的显示文案 (导入工作区 / 应用模板 / 初始状态)。
export function rebaseHistory(baselineOpKey?: string): void {
  const entries = makeBaseline(baselineOpKey);
  useHistoryStore.setState({ entries, index: 0, ...derivedFlags(entries, 0) });
  lastCoalesceKey = null;
  lastCoalesceAt = 0;
}

/// 控件类型 → 已有 i18n 键 (面板列表补充信息用); 未映射的类型不显示补充信息
export function widgetKindLabelKey(kind: WidgetConfig['kind']): string | null {
  switch (kind) {
    case 'Waveform':
      return 'dataTabWaveform';
    case 'Spectrum':
      return 'dataTabSpectrum';
    case 'RawData':
      return 'dataTabRawData';
    case 'PieChart':
      return 'dataTabPie';
    case 'Image':
      return 'dataTabImage';
    case 'Model3D':
      return 'dataTabModel3d';
    case 'Command':
      return 'dataTabCommand';
    case 'FrameDecoder':
      return 'dataTabFrameDecoder';
    case 'Trigger':
      return 'dataTabTrigger';
    default:
      return null;
  }
}
