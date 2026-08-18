import type { WidgetBinding } from '../../types';
import { useAppStore } from '../../store/appStore';
import { downstreamProtocolOf, isGlobalNode } from '../../store/appStoreHelpers';

/// 解析控件发送的目标: [transportNodeId, protocolNodeId | null]
/// - transport: 第一个全局 Transport 节点
/// - protocol: 该 Transport 字节边下游的 Protocol 节点, 缺省第一个 Protocol 节点
function resolveSendTarget(): [string | null, string | null] {
  const s = useAppStore.getState();
  const transport = s.rfNodes.find((n: any) => n.type === 'transport' && isGlobalNode(n));
  if (!transport) return [null, null];
  const downstream = downstreamProtocolOf(transport.id, s.rfEdges, s.rfNodes);
  if (downstream) return [transport.id, downstream];
  const protocol = s.rfNodes.find((n: any) => n.type === 'protocol' && isGlobalNode(n));
  return [transport.id, protocol?.id ?? null];
}

/// 根据绑定模式发送控件值
/// - None: 不发送
/// - Auto: 调用后端 encode_channel(channel, value) (protocolNode 取目标 Transport 下游的 Protocol 节点)
/// - Manual: 使用模板 {value} 替换后以字符串发送
export function sendBindingValue(binding: WidgetBinding, value: number) {
  const state = useAppStore.getState();
  const [transportId, protocolId] = resolveSendTarget();
  if (!transportId) return;

  switch (binding.mode) {
    case 'None':
      return;
    case 'Auto': {
      if (!protocolId) return;
      const protocolNode = state.rfNodes.find((n: any) => n.id === protocolId);
      const config = protocolNode?.data?.config as { kind?: string } | undefined;
      if (config?.kind === 'RawData') return;
      state.sendWidgetValue(transportId, protocolId, binding, value);
      return;
    }
    case 'Manual':
      state.sendText(transportId, binding.params.template.replace(/\{value\}/g, String(value)));
      return;
  }
}
