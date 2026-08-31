/// 图编译派生数据 store — 后端 `cmd_graph::GraphDerived` 镜像
///
/// 由 `update_tab_graph` 响应 (Promise resolve) 与 `graph:derived` 事件共同驱动写入,
/// 前端 React Flow handle 渲染与节点摘要改从此 store 读取, 消除前端预言后端状态的
/// 整个类别 (协议切换 / 模板替换 / 自动检测变化)。来源:
/// - Tauri 命令 `update_tab_graph` 的响应 (节点全量替换)
/// - `graph:derived` 事件 (按节点级别差分推送)

import type { AppSlice } from './types';

export type DerivedPortDomain = 'F32' | 'Bytes' | 'String';

export interface NodeDerivedPort {
  name: string;
  domain: DerivedPortDomain;
}

export interface NodeDerived {
  ports: NodeDerivedPort[];
  /// 仅 Protocol 节点有意义; 手动配置值或自动检测值
  effective_channels?: number;
}

/// 与后端 `cmd_graph::GraphDerived` 同形 (前端不消费 nodeId, 仅按 nodeId 索引)
export interface GraphDerivedPayload {
  nodes: {
    node_id: string;
    ports: NodeDerivedPort[];
    effective_channels?: number | null;
  }[];
  /// 提交成功后的全局图版本号 (前端下次提交作为 base_version 冲突检测基线)
  version?: number;
}

export interface DerivedSlice {
  /// 按节点 id 索引的派生端口表 / 通道数 (后端单一权威)
  derivedPorts: Record<string, NodeDerived>;
  /// 写入一组节点派生数据 (按 node_id 合并)
  setDerived: (nodes: GraphDerivedPayload['nodes']) => void;
  /// 清理一组节点的派生数据 (节点被移除时调用)
  removeDerived: (nodeIds: string[]) => void;
  /// 重置整张表 (snapshot 导入/恢复出厂等场景)
  resetDerived: () => void;
}

export const createDerivedSlice: AppSlice<DerivedSlice> = (set, _get) => {
  return {
    derivedPorts: {},

    setDerived: (nodes) =>
      set((s) => {
        const next = { ...s.derivedPorts };
        for (const n of nodes) {
          next[n.node_id] = {
            ports: n.ports,
            effective_channels:
              n.effective_channels == null ? undefined : n.effective_channels,
          };
        }
        return { derivedPorts: next };
      }),

    removeDerived: (nodeIds) =>
      set((s) => {
        if (nodeIds.length === 0) return {};
        const next = { ...s.derivedPorts };
        for (const id of nodeIds) delete next[id];
        return { derivedPorts: next };
      }),

    resetDerived: () => set({ derivedPorts: {} }),
  };
}
