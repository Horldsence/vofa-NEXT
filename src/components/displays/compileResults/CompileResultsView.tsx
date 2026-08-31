import { memo, useEffect, useMemo, useState } from 'react';
import { useAppStore } from '../../../store/appStore';
import { syncTabGraphToBackend } from '../../../store/appStoreHelpers';
import {
  edgeClassLabel,
  isDualRole,
  type EdgeClass,
  type GraphHir,
  type HirEdgeView,
  type HirNodeView,
  type PortDomain,
} from '../../../store/slices/compileHir';
import { activateOnKeyboard } from '../../../lib/utils/a11y';
import {
  Trash2,
  Copy as CopyIcon,
  Plus as PlusIcon,
  Edit2,
  ClipboardPaste,
  X as XIcon,
} from 'lucide-react';
import { t } from '../../../i18n';
import type { Edge } from '@xyflow/react';

/// Add / Modify Edge 模态框状态
interface EdgeModalState {
  open: boolean;
  mode: 'add' | 'modify';
  edge?: HirEdgeView;
  // 表单字段
  sourceNode: string;
  sourceHandle: string;
  targetNode: string;
  targetHandle: string;
}

const EMPTY_MODAL: EdgeModalState = {
  open: false,
  mode: 'add',
  sourceNode: '',
  sourceHandle: '',
  targetNode: '',
  targetHandle: '',
};

/// 端口域 chip 颜色 (按域)
const DOMAIN_CHIP_CLASS: Record<PortDomain, string> = {
  F32: 'bg-blue-500/20 text-blue-300 border-blue-500/40',
  Bytes: 'bg-green-500/20 text-green-300 border-green-500/40',
  String: 'bg-amber-500/20 text-amber-300 border-amber-500/40',
};

/// 边分类 badge 颜色
function edgeClassChipClass(c: EdgeClass): string {
  switch (c.kind) {
    case 'byte':
      return 'bg-green-500/20 text-green-300 border-green-500/40';
    case 'f32':
      return 'bg-blue-500/20 text-blue-300 border-blue-500/40';
    case 'str':
      return 'bg-amber-500/20 text-amber-300 border-amber-500/40';
    case 'raw_data_marker':
      return 'bg-zinc-500/30 text-zinc-300 border-zinc-500/40';
  }
}

export const CompileResultsView = memo(function CompileResultsView() {
  const lang = useAppStore((s) => s.lang);
  const activeControlTabId = useAppStore((s) => s.activeControlTabId);
  const rfNodes = useAppStore((s) => s.rfNodes);
  const onEdgesChange = useAppStore((s) => s.onEdgesChange);
  const requestFlyTo = useAppStore((s) => s.requestFlyTo);
  const hirLoading = useAppStore((s) => s.hirLoading);
  const fetchHir = useAppStore((s) => s.fetchHir);

  // 编译错误状态 (与 NodeEditor 同源: compileStatus 切片, 由 graph:compile 事件写入)
  const tabState = useAppStore((s) =>
    activeControlTabId ? s.tabStates[activeControlTabId] : undefined
  );
  const EMPTY_NODES: readonly string[] = [];
  const EMPTY_EDGES: readonly string[] = [];
  const tabErrorNodes = useAppStore((s) =>
    activeControlTabId ? s.tabErrorNodes[activeControlTabId] ?? EMPTY_NODES : EMPTY_NODES
  );
  const tabErrorEdges = useAppStore((s) =>
    activeControlTabId ? s.tabErrorEdges[activeControlTabId] ?? EMPTY_EDGES : EMPTY_EDGES
  );
  // 仅在 tab 处于 error 态时高亮, 避免 ok 态误标 (compileStatus 在 ok 时不清错误列表,
  // 残留历史需靠 tabState 门控)
  const erroredEdgeIds = useMemo(
    () => (tabState === 'error' ? new Set(tabErrorEdges) : new Set<string>()),
    [tabErrorEdges, tabState]
  );
  const erroredNodeIds = useMemo(
    () => (tabState === 'error' ? new Set(tabErrorNodes) : new Set<string>()),
    [tabErrorNodes, tabState]
  );

  const hir: GraphHir | null =
    useAppStore((s) => (activeControlTabId ? s.hirByTab[activeControlTabId] : null)) ?? null;

  // 切到画布 Tab — 点击高亮时用, 让 NodeEditor 挂载并消费 flyToRequest
  const setActiveControlTab = useAppStore((s) => s.setActiveControlTab);
  // 持久画布高亮 — 与 highlightedNodeId 同步, 节点组件订阅并加 accent 边框
  const setCanvasHighlight = useAppStore((s) => s.setCanvasHighlight);
  const clearCanvasHighlight = useAppStore((s) => s.clearCanvasHighlight);

  /// 挂载 / 切 tab 时拉取 HIR (兜底, 主路径在 events.ts 监听 graph:compile 后 refetch)
  useEffect(() => {
    if (activeControlTabId) void fetchHir(activeControlTabId);
  }, [activeControlTabId, fetchHir]);

  const [clipboardEdge, setClipboardEdge] = useState<HirEdgeView | null>(null);
  const [modal, setModal] = useState<EdgeModalState>(EMPTY_MODAL);

  /// 点击高亮的静态状态 — 单选; 再次点击同节点取消; 切 tab 时清空
  const [highlightedNodeId, setHighlightedNodeId] = useState<string | null>(null);
  useEffect(() => {
    // 切 tab 时清空选中, 避免旧 tab 的高亮残留到新 tab
    setHighlightedNodeId(null);
    clearCanvasHighlight();
  }, [activeControlTabId, clearCanvasHighlight]);

  const nodeById = useMemo(() => {
    const m = new Map<string, HirNodeView>();
    if (hir) for (const n of hir.nodes) m.set(n.nodeId, n);
    return m;
  }, [hir]);

  const getNodeName = (id: string): string => {
    const node = rfNodes.find((n) => n.id === id);
    if (!node) return id;
    if (node.type === 'transport') return t(lang, 'nodeTypeTransport');
    if (node.type === 'protocol') return t(lang, 'nodeTypeProtocol');
    const widget = (node.data as { widget?: { params?: { label?: string }; kind?: string } } | undefined)?.widget;
    return (widget?.params?.label ?? widget?.kind) ?? id;
  };

  const handleHighlight = (nodeId: string) => {
    const node = rfNodes.find((n) => n.id === nodeId);
    if (!node) return;
    const tabId = (node.data as { tabId?: string } | undefined)?.tabId ?? activeControlTabId ?? 'main';
    // 1. 表格内静态高亮 + 画布高亮同步 — 单选, 再次点同节点取消
    setHighlightedNodeId((prev) => {
      if (prev === nodeId) {
        clearCanvasHighlight();
        return null;
      }
      setCanvasHighlight(nodeId, tabId);
      return nodeId;
    });
    // 2. 切到画布 Tab (NodeEditor 挂载) — 让 flyToRequest 在画布生效
    if (activeControlTabId) setActiveControlTab(activeControlTabId);
    // 3. 写入 flyToRequest — NodeEditor 的 useEffect 命中即调 reactFlow.setCenter
    requestFlyTo(nodeId, tabId);
  };

  /// 提交 CRUD 变更: 修改本地 rfEdges → syncTabGraphToBackend → 后端 emit graph:compile →
  /// events.ts 监听器调 fetchHir → store 更新 → 表格重渲染
  const submitChange = async () => {
    if (!activeControlTabId) return;
    await syncTabGraphToBackend(activeControlTabId);
  };

  const handleAdd = () => {
    const firstNode = hir?.nodes[0]?.nodeId ?? '';
    setModal({
      open: true,
      mode: 'add',
      sourceNode: firstNode,
      sourceHandle: '',
      targetNode: hir?.nodes[1]?.nodeId ?? '',
      targetHandle: '',
    });
  };

  const handleModify = (edge: HirEdgeView) => {
    setModal({
      open: true,
      mode: 'modify',
      edge,
      sourceNode: edge.sourceNode,
      sourceHandle: edge.sourceHandle,
      targetNode: edge.targetNode,
      targetHandle: edge.targetHandle,
    });
  };

  const handleCopy = (edge: HirEdgeView) => {
    setClipboardEdge({ ...edge });
  };

  const handlePaste = async () => {
    if (!clipboardEdge) return;
    const newEdge: Edge = {
      id: `e-${Date.now()}`,
      source: clipboardEdge.sourceNode,
      sourceHandle: clipboardEdge.sourceHandle,
      target: clipboardEdge.targetNode,
      targetHandle: clipboardEdge.targetHandle,
    };
    onEdgesChange([{ type: 'add', item: newEdge }]);
    await submitChange();
  };

  const handleDelete = async (edgeId: string) => {
    onEdgesChange([{ type: 'remove', id: edgeId }]);
    await submitChange();
  };

  const closeModal = () => setModal(EMPTY_MODAL);

  const submitModal = async () => {
    if (!modal.sourceNode || !modal.targetNode) return;
    if (modal.mode === 'add') {
      const newEdge: Edge = {
        id: `e-${Date.now()}`,
        source: modal.sourceNode,
        sourceHandle: modal.sourceHandle || undefined,
        target: modal.targetNode,
        targetHandle: modal.targetHandle || undefined,
      };
      onEdgesChange([{ type: 'add', item: newEdge }]);
    } else if (modal.mode === 'modify' && modal.edge) {
      const oldId = modal.edge.edgeId;
      const updated: Edge = {
        id: oldId,
        source: modal.sourceNode,
        sourceHandle: modal.sourceHandle || undefined,
        target: modal.targetNode,
        targetHandle: modal.targetHandle || undefined,
      };
      onEdgesChange([{ type: 'remove', id: oldId }, { type: 'add', item: updated }]);
    }
    closeModal();
    await submitChange();
  };

  const edges = hir?.edges ?? [];
  const hasAnyEdge = edges.length > 0;

  return (
    <div className="flex flex-col h-full w-full bg-bg-editor p-4">
      <div className="flex flex-wrap justify-between items-center mb-3 gap-2">
        <h2 className="text-sm font-semibold text-text-primary">
          {t(lang, 'connectionResultsTitle')}
        </h2>
        <div className="flex gap-2 flex-wrap">
          <button
            className="flex items-center gap-1 px-2 py-1 bg-bg-button hover:bg-bg-hover text-text-primary rounded text-xs disabled:opacity-50"
            onClick={handleAdd}
            disabled={hirLoading || !hir || (hir.nodes?.length ?? 0) < 2}
          >
            <PlusIcon size={14} /> {t(lang, 'addConnection')}
          </button>
          <button
            className="flex items-center gap-1 px-2 py-1 bg-bg-button hover:bg-bg-hover text-text-primary rounded text-xs disabled:opacity-50"
            onClick={() => { void handlePaste(); }}
            disabled={!clipboardEdge || hirLoading}
          >
            <ClipboardPaste size={14} /> {t(lang, 'pasteConnection')}
          </button>
        </div>
      </div>

      {/* 列显隐勾选已移除 — 默认全部显示 */}

      <div className="flex-1 overflow-auto border border-border rounded" data-tour="results-table">
        <table className="w-full text-left text-xs text-text-secondary">
          <thead className="bg-bg-panel-header sticky top-0 border-b border-border">
            <tr>
              <th className="px-4 py-2 font-medium">{t(lang, 'colSource')}</th>
              <th className="px-4 py-2 font-medium">{t(lang, 'colTarget')}</th>
              <th className="px-4 py-2 font-medium">{t(lang, 'colSourceDomain')}</th>
              <th className="px-4 py-2 font-medium">{t(lang, 'colTargetDomain')}</th>
              <th className="px-4 py-2 font-medium">{t(lang, 'colEdgeClass')}</th>
              <th className="px-4 py-2 font-medium">{t(lang, 'colDualRole')}</th>
              <th className="px-4 py-2 font-medium text-right">{t(lang, 'colActions')}</th>
            </tr>
          </thead>
          <tbody>
            {!hasAnyEdge ? (
              <tr>
                <td colSpan={7} className="px-4 py-8 text-center text-text-muted">
                  {t(lang, 'noConnections')}
                </td>
              </tr>
            ) : (
              edges.map((edge) => {
                const sourceNode = nodeById.get(edge.sourceNode);
                const targetNode = nodeById.get(edge.targetNode);
                const sourceDual = sourceNode ? isDualRole(sourceNode) : false;
                const targetDual = targetNode ? isDualRole(targetNode) : false;
                const isDual = sourceDual || targetDual;
                // 复用 compileStatus tabErrorEdges / tabErrorNodes 高亮 — 与 NodeEditor
                // 画布红框 + CanvasErrorTooltip  ring-2  ring-red-500 一致
                const isErroredEdge = erroredEdgeIds.has(edge.edgeId);
                const isErroredSource = erroredNodeIds.has(edge.sourceNode);
                const isErroredTarget = erroredNodeIds.has(edge.targetNode);
                const isErrored = isErroredEdge || isErroredSource || isErroredTarget;
                // 用户点击高亮的源/目标节点 — 该行用蓝色 ring + bg 静态标记
                // (单选, 再次点击同节点取消; 切 tab 自动清空)
                const isHighlightedSource = highlightedNodeId === edge.sourceNode;
                const isHighlightedTarget = highlightedNodeId === edge.targetNode;
                const isHighlighted = isHighlightedSource || isHighlightedTarget;
                const rowClass = isErrored
                  ? 'bg-red-500/10 ring-1 ring-red-500/60'
                  : isHighlighted
                    ? 'bg-blue-500/15 ring-1 ring-blue-500/60'
                    : 'hover:bg-bg-sidebar';
                return (
                  <tr
                    key={edge.edgeId}
                    className={`border-b border-border-subtle transition-colors ${rowClass}`}
                  >
                    <td className="px-4 py-2">
                      <div className="flex flex-col gap-1">
                        <button
                          type="button"
                          className={`font-medium cursor-pointer hover:underline ${
                            isErroredSource ? 'text-red-400' : 'text-text-primary'
                          }`}
                          onClick={() => handleHighlight(edge.sourceNode)}
                          onKeyDown={activateOnKeyboard}
                        >
                          {getNodeName(edge.sourceNode)}
                        </button>
                        <span className="text-text-muted font-mono bg-bg-input px-1 rounded inline-block w-fit">
                          {edge.sourceHandle || t(lang, 'defaultHandle')}
                        </span>
                      </div>
                    </td>
                    <td className="px-4 py-2">
                      <div className="flex flex-col gap-1">
                        <button
                          type="button"
                          className={`font-medium cursor-pointer hover:underline ${
                            isErroredTarget ? 'text-red-400' : 'text-text-primary'
                          }`}
                          onClick={() => handleHighlight(edge.targetNode)}
                          onKeyDown={activateOnKeyboard}
                        >
                          {getNodeName(edge.targetNode)}
                        </button>
                        <span className="text-text-muted font-mono bg-bg-input px-1 rounded inline-block w-fit">
                          {edge.targetHandle || t(lang, 'defaultHandle')}
                        </span>
                      </div>
                    </td>
                    <td className="px-4 py-2">
                      <span
                        className={`px-2 py-0.5 rounded text-[10px] font-mono border ${DOMAIN_CHIP_CLASS[edge.sourceDomain]}`}
                      >
                        {edge.sourceDomain}
                      </span>
                    </td>
                    <td className="px-4 py-2">
                      <span
                        className={`px-2 py-0.5 rounded text-[10px] font-mono border ${DOMAIN_CHIP_CLASS[edge.targetDomain]}`}
                      >
                        {edge.targetDomain}
                      </span>
                    </td>
                    <td className="px-4 py-2">
                      <span
                        className={`px-2 py-0.5 rounded text-[10px] font-mono border ${
                          isErroredEdge
                            ? 'bg-red-500/20 text-red-300 border-red-500/60'
                            : edgeClassChipClass(edge.class)
                        }`}
                      >
                        {edgeClassLabel(edge.class)}
                      </span>
                    </td>
                    <td className="px-4 py-2">
                      {isDual ? (
                        <span className="px-2 py-0.5 rounded text-[10px] font-mono border bg-purple-500/20 text-purple-300 border-purple-500/40">
                          {t(lang, 'dualRoleMarker')}
                        </span>
                      ) : (
                        <span className="text-text-muted">—</span>
                      )}
                    </td>
                    <td className="px-4 py-2">
                      <div className="flex justify-end gap-2">
                        <button
                          className="p-1 text-text-muted hover:text-text-primary transition-colors"
                          title={t(lang, 'editConnection')}
                          onClick={() => handleModify(edge)}
                          disabled={hirLoading}
                        >
                          <Edit2 size={14} />
                        </button>
                        <button
                          className="p-1 text-text-muted hover:text-text-primary transition-colors"
                          title={t(lang, 'contextMenuCopy')}
                          onClick={() => handleCopy(edge)}
                          disabled={hirLoading}
                        >
                          <CopyIcon size={14} />
                        </button>
                        <button
                          className="p-1 text-red-500 hover:text-red-400 transition-colors"
                          title={t(lang, 'contextMenuDelete')}
                          onClick={() => { void handleDelete(edge.edgeId); }}
                          disabled={hirLoading}
                        >
                          <Trash2 size={14} />
                        </button>
                      </div>
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>

      {/* Add / Modify Edge 模态框 */}
      {modal.open && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
          <div className="bg-bg-panel border border-border rounded-lg shadow-lg w-[420px] p-4">
            <div className="flex justify-between items-center mb-3">
              <h3 className="text-sm font-semibold text-text-primary">
                {modal.mode === 'add'
                  ? t(lang, 'modalAddEdge')
                  : t(lang, 'modalModifyEdge')}
              </h3>
              <button
                className="p-1 text-text-muted hover:text-text-primary"
                onClick={closeModal}
              >
                <XIcon size={14} />
              </button>
            </div>
            <div className="flex flex-col gap-2 text-xs">
              <label className="flex flex-col gap-1">
                <span className="text-text-secondary">{t(lang, 'fieldSourceNode')}</span>
                <select
                  className="bg-bg-input border border-border rounded px-2 py-1 text-text-primary"
                  value={modal.sourceNode}
                  onChange={(e) =>
                    setModal((m) => ({ ...m, sourceNode: e.target.value }))
                  }
                >
                  {(hir?.nodes ?? []).map((n) => (
                    <option key={n.nodeId} value={n.nodeId}>
                      {getNodeName(n.nodeId)}
                    </option>
                  ))}
                </select>
              </label>
              <label className="flex flex-col gap-1">
                <span className="text-text-secondary">{t(lang, 'fieldSourceHandle')}</span>
                <input
                  type="text"
                  className="bg-bg-input border border-border rounded px-2 py-1 text-text-primary font-mono"
                  value={modal.sourceHandle}
                  onChange={(e) =>
                    setModal((m) => ({ ...m, sourceHandle: e.target.value }))
                  }
                  placeholder="value / result / ch0..."
                />
              </label>
              <label className="flex flex-col gap-1">
                <span className="text-text-secondary">{t(lang, 'fieldTargetNode')}</span>
                <select
                  className="bg-bg-input border border-border rounded px-2 py-1 text-text-primary"
                  value={modal.targetNode}
                  onChange={(e) =>
                    setModal((m) => ({ ...m, targetNode: e.target.value }))
                  }
                >
                  {(hir?.nodes ?? []).map((n) => (
                    <option key={n.nodeId} value={n.nodeId}>
                      {getNodeName(n.nodeId)}
                    </option>
                  ))}
                </select>
              </label>
              <label className="flex flex-col gap-1">
                <span className="text-text-secondary">{t(lang, 'fieldTargetHandle')}</span>
                <input
                  type="text"
                  className="bg-bg-input border border-border rounded px-2 py-1 text-text-primary font-mono"
                  value={modal.targetHandle}
                  onChange={(e) =>
                    setModal((m) => ({ ...m, targetHandle: e.target.value }))
                  }
                  placeholder="in0 / in1 / str..."
                />
              </label>
            </div>
            <div className="flex justify-end gap-2 mt-4">
              <button
                className="px-3 py-1 bg-bg-button hover:bg-bg-hover text-text-primary rounded text-xs"
                onClick={closeModal}
              >
                {t(lang, 'cancel')}
              </button>
              <button
                className="px-3 py-1 bg-accent-primary hover:opacity-80 text-text-primary rounded text-xs disabled:opacity-50"
                onClick={() => { void submitModal(); }}
                disabled={
                  !modal.sourceNode ||
                  !modal.targetNode ||
                  modal.sourceNode === modal.targetNode
                }
              >
                {t(lang, 'confirmAdd')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
});
