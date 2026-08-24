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
  setCompileEvent: (e: GraphCompileEvent) => void;
  resetStatus: (tabId?: string) => void;
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
        } else if (e.state === 'ok') {
          delete nextErrors[tabId];
          delete nextErrorNodes[tabId];
          delete nextErrorEdges[tabId];
        }
        const pending = Object.entries(nextStates)
          .filter(([, v]) => v === 'pending' || v === 'compiling')
          .map(([k]) => k);
        const errors = Object.entries(nextStates)
          .filter(([, v]) => v === 'error')
          .map(([k]) => k);
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
        const errors = Object.entries(nextStates)
          .filter(([, v]) => v === 'error')
          .map(([k]) => k);
        return {
          tabStates: nextStates,
          tabErrors: restErrors,
          tabErrorNodes: restNodes,
          tabErrorEdges: restEdges,
          globalErrors: restGlobal,
          pendingTabs: pending,
          errorTabs: errors,
          anyCompiling: pending.length > 0,
        };
      }),
  };
}
