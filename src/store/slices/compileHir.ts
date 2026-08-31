/// HIR 切片 — 后端 `cmd_graph::hir_query` 通过 `get_graph_hir` 命令按需拉取的 HIR 视图
///
/// 用途: `compile-results` Tab 改用后端编译产物展示 (边分类 / 端口域 / 节点双角色),
/// 而不是直接读 ReactFlow `rfEdges`.
///
/// 数据流:
/// - 组件挂载 / 切 tab: `fetchHir(tabId)` → `invoke('get_graph_hir')` → `hirByTab[tabId]`
/// - 后端 `apply_tab_graph` 编译成功 → emit `graph:compile state=Ok` →
///   `events.ts` 监听器调 `get().fetchHir(tab_id)` 触发 refetch
/// - 错误态 (state=error) **不** refetch — 保留上次成功 HIR,
///   错误详情由 `compile-errors` Tab 单独展示

import { invoke } from '@tauri-apps/api/core';
import type { Edge } from '@xyflow/react';

// ============ 类型 (mirror cmd_graph::hir_query::GraphHir, camelCase JSON) ============

export type PortDomain = 'F32' | 'Bytes' | 'String';

export type EdgeClass =
  | { kind: 'byte' }
  | { kind: 'f32' }
  | { kind: 'str' }
  | { kind: 'raw_data_marker'; sourceDomain: PortDomain };

export interface HirNodeView {
  nodeId: string;
  hasValueDef: boolean;
  hasByteDef: boolean;
}

export interface HirEdgeView {
  edgeId: string;
  sourceNode: string;
  sourceHandle: string;
  sourceDomain: PortDomain;
  targetNode: string;
  targetHandle: string;
  targetDomain: PortDomain;
  class: EdgeClass;
}

export interface GraphHir {
  tabId: string;
  nodes: HirNodeView[];
  edges: HirEdgeView[];
}

// ============ Zustand slice (与现有 slices 同模式) ============

import type { AppSlice } from './types';

export interface CompileHirSlice {
  /// 按 tabId 缓存 HIR 视图; 缺失视为未编译
  hirByTab: Record<string, GraphHir | null>;
  /// 拉取中标志 (用于禁用 CRUD 按钮避免竞态)
  hirLoading: boolean;
  fetchHir: (tabId: string) => Promise<void>;
}

export const createCompileHirSlice: AppSlice<CompileHirSlice> = (set, _get) => {
  return {
    hirByTab: {},
    hirLoading: false,

    fetchHir: async (tabId: string) => {
      set({ hirLoading: true });
      try {
        const hir = await invoke<GraphHir>('get_graph_hir', { tabId });
        set((s: any) => ({
          hirByTab: { ...s.hirByTab, [tabId]: hir },
          hirLoading: false,
        }));
      } catch (e) {
        console.error('[compileHir] fetchHir failed:', e);
        set({ hirLoading: false });
      }
    },
  };
}

// ============ 辅助函数 (组件 / 事件监听器复用) ============

/** 把 HIR 边列表转回 Edge[] (给 `update_tab_graph` 命令用) */
export function convertHirToRfEdges(hir: GraphHir | null | undefined): Edge[] {
  if (!hir) return [];
  return hir.edges.map((e) => ({
    id: e.edgeId,
    source: e.sourceNode,
    sourceHandle: e.sourceHandle,
    target: e.targetNode,
    targetHandle: e.targetHandle,
  }));
}

/** 节点是否双角色 (同时有 value_def 和 byte_def — 例如 ProtocolSource + Protocol 同 id 共存) */
export function isDualRole(n: HirNodeView): boolean {
  return n.hasValueDef && n.hasByteDef;
}

/** EdgeClass 可读 label (UI 显示用) */
export function edgeClassLabel(c: EdgeClass): string {
  switch (c.kind) {
    case 'byte':
      return 'Byte';
    case 'f32':
      return 'F32';
    case 'str':
      return 'Str';
    case 'raw_data_marker':
      return `Raw(${c.sourceDomain})`;
  }
}
