//! 全局节点查询 hooks — 多数据源并存时各视图取"主"节点 (第一个对应类型的全局节点)
import { useAppStore } from '../../store/appStore';
import { isGlobalNode, type ProtocolNodeData, type TransportNodeData } from '../../store/appStoreHelpers';
import type { ProtocolConfig, TransportConfig } from '../../types';

/// 第一个全局 Protocol 节点的配置 (无 Protocol 节点时 null)
export function usePrimaryProtocolConfig(): ProtocolConfig | null {
  return useAppStore((s) => {
    const n = s.rfNodes.find((x) => x.type === 'protocol' && isGlobalNode(x));
    return n ? (n.data as unknown as ProtocolNodeData).config : null;
  });
}

/// 第一个全局 Transport 节点的配置 (无 Transport 节点时 null)
export function usePrimaryTransportConfig(): TransportConfig | null {
  return useAppStore((s) => {
    const n = s.rfNodes.find((x) => x.type === 'transport' && isGlobalNode(x));
    return n ? (n.data as unknown as TransportNodeData).config : null;
  });
}

/// 第一个全局 Transport 节点 id (无则 null)
export function usePrimaryTransportNodeId(): string | null {
  return useAppStore((s) => {
    const n = s.rfNodes.find((x) => x.type === 'transport' && isGlobalNode(x));
    return n?.id ?? null;
  });
}

/// 第一个全局 Protocol 节点 id (无则 null)
export function usePrimaryProtocolNodeId(): string | null {
  return useAppStore((s) => {
    const n = s.rfNodes.find((x) => x.type === 'protocol' && isGlobalNode(x));
    return n?.id ?? null;
  });
}

/// 第一个 CAN 能力 Transport 节点 id (Slcan/CandleLight 优先, 否则第一个 Transport, 无则 null)
/// 供 CAN 负载/比特率命令的 nodeId 参数使用
export function usePrimaryCanTransportNodeId(): string | null {
  return useAppStore((s) => {
    const transports = s.rfNodes.filter((x) => x.type === 'transport' && isGlobalNode(x));
    const canNode = transports.find((x) => {
      const kind = (x.data as unknown as TransportNodeData).config?.kind;
      return kind === 'Slcan' || kind === 'CandleLight';
    });
    return (canNode ?? transports[0])?.id ?? null;
  });
}
