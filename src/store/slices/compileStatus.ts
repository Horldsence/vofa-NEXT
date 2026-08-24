/// 编译状态切片 — 后端 `cmd_graph` 通过 `graph:compile` 事件广播的 tab 编译状态.
///
/// 字段:
/// - `tabStates`: tab_id → 当前状态 ('ok' | 'pending' | 'compiling' | 'error')
/// - `tabErrors`: tab_id → 最近一次错误的报告
/// - `tabErrorNodes`: tab_id → 受影响节点 id 列表 (画布红框高亮)
/// - `tabErrorEdges`: tab_id → 受影响边 id 列表
/// - `globalErrors`: tab_id → 完整事件 payload (供错误面板浏览)
/// - `anyCompiling`: 全局是否有任意 tab 处于 pending/compiling (状态栏指示)
/// - `pendingTabs` / `errorTabs`: 按状态分组的 tab id 集合 (供 tab 角标批量读取)

import type { CompileReport } from './compileError';

export type TabCompileState = 'ok' | 'pending' | 'compiling' | 'error';

export interface GraphCompileEvent {
  tab_id: string;
  state: TabCompileState;
  queued_seq: number;
  report: CompileReport | null;
}

export interface CompileStatusSlice {
  tabStates: Record<string, TabCompileState>;
  tabErrors: Record<string, CompileReport>;
  tabErrorNodes: Record<string, string[]>;
  tabErrorEdges: Record<string, string[]>;
  globalErrors: Record<string, GraphCompileEvent>;
  pendingTabs: string[];
  errorTabs: string[];
  anyCompiling: boolean;
  /// 一次性 fly-to 请求 — CompileErrorItem 触发, NodeEditorInner 命中即消费并清空
  flyToRequest: { nodeId: string; tabId: string } | null;
  setCompileEvent: (e: GraphCompileEvent) => void;
  resetStatus: (tabId?: string) => void;
  requestFlyTo: (nodeId: string, tabId: string) => void;
  clearFlyToRequest: () => void;
}

export function createCompileStatusSlice(set: any, _get: any): CompileStatusSlice {
  return {
    tabStates: {},
    tabErrors: {},
    tabErrorNodes: {},
    tabErrorEdges: {},
    globalErrors: {},
    pendingTabs: [],
    errorTabs: [],
    anyCompiling: false,
    flyToRequest: null,

    setCompileEvent: (e) =>
      set((s: any) => {
        const tabId = e.tab_id;
        const nextStates = { ...s.tabStates, [tabId]: e.state };
        const nextErrors = { ...s.tabErrors };
        const nextErrorNodes = { ...s.tabErrorNodes };
        const nextErrorEdges = { ...s.tabErrorEdges };
        const nextGlobal = { ...s.globalErrors, [tabId]: e };
        if (e.state === 'error' && e.report) {
          nextErrors[tabId] = e.report;
          nextErrorNodes[tabId] = e.report.nodes ?? [];
          nextErrorEdges[tabId] = e.report.edges ?? [];
        }
        // 注意: state === 'ok' 时不删除 nextErrors/nextErrorNodes/nextErrorEdges —
        // 保留历史供 Compile Errors 面板回放 (用户确认: 错误修复后状态栏图标不消失,
        // tab 分组显示绿色 ✓ 等待用户手动 X 关闭).
        // 真正的清理靠 resetStatus(tabId) — 由 removeControlTab / removeDataTab 触发
        const pending = Object.entries(nextStates)
          .filter(([, v]) => v === 'pending' || v === 'compiling')
          .map(([k]) => k);
        // errorTabs: 累积式 — 一旦进入 error 的 tabId 永不退出, 除非 resetStatus
        const errorSet = new Set(s.errorTabs);
        if (e.state === 'error') errorSet.add(tabId);
        const errors = Array.from(errorSet);
        return {
          tabStates: nextStates,
          tabErrors: nextErrors,
          tabErrorNodes: nextErrorNodes,
          tabErrorEdges: nextErrorEdges,
          globalErrors: nextGlobal,
          pendingTabs: pending,
          errorTabs: errors,
          anyCompiling: pending.length > 0,
        };
      }),

    resetStatus: (tabId) =>
      set((s: any) => {
        if (tabId === undefined) {
          return {
            tabStates: {},
            tabErrors: {},
            tabErrorNodes: {},
            tabErrorEdges: {},
            globalErrors: {},
            pendingTabs: [],
            errorTabs: [],
            anyCompiling: false,
            flyToRequest: null,
          };
        }
        const { [tabId]: _, ...restStates } = s.tabStates;
        const nextStates = restStates;
        const { [tabId]: __, ...restErrors } = s.tabErrors;
        const { [tabId]: ___, ...restNodes } = s.tabErrorNodes;
        const { [tabId]: ____, ...restEdges } = s.tabErrorEdges;
        const { [tabId]: _____, ...restGlobal } = s.globalErrors;
        const pending = Object.entries(nextStates)
          .filter(([, v]) => v === 'pending' || v === 'compiling')
          .map(([k]) => k);
        // resetStatus 同步把 errorTabs 里这个 tabId 移除 — 与"累积式"语义配合
        // (累积是相对 setCompileEvent 的; tab 整体删除时显式清理)
        const errors = s.errorTabs.filter((id: string) => id !== tabId);
        return {
          tabStates: nextStates,
          tabErrors: restErrors,
          tabErrorNodes: restNodes,
          tabErrorEdges: restEdges,
          globalErrors: restGlobal,
          pendingTabs: pending,
          errorTabs: errors,
          anyCompiling: pending.length > 0,
          // 若当前 fly-to 请求指向已删 tab, 同步清掉避免孤儿
          flyToRequest:
            s.flyToRequest && s.flyToRequest.tabId === tabId ? null : s.flyToRequest,
        };
      }),

    /// 排队 fly-to 请求 — CompileErrorItem 在切到 control tab 后调用,
    /// NodeEditorInner 的 useEffect 命中即调 reactFlow.setCenter 并消费
    requestFlyTo: (nodeId, tabId) => set({ flyToRequest: { nodeId, tabId } }),

    /// 消费方 (NodeEditorInner) 命中后调, 也可由超时/手动重置触发
    clearFlyToRequest: () => set({ flyToRequest: null }),
  };
}
