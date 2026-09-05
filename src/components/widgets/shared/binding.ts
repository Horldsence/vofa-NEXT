import type { WidgetBinding } from '../../../types';
import { useAppStore } from '../../../store/appStore';

/// 根据绑定模式发送控件值
/// - None: 不发送
/// - Auto: 调用后端 encode_channel(channel, value) (protocolNode 取目标 Transport 下游的 Protocol 节点)
/// - Manual: 使用模板 {value} 替换后以字符串发送
export function sendBindingValue(binding: WidgetBinding, value: number) {
  const state = useAppStore.getState();

  switch (binding.mode) {
    case 'None':
      return;
    case 'Auto': {
      const { transportId, protocolId } = binding.params;
      const transportNode = state.rfNodes.find((n) => n.id === transportId && n.type === 'transport');
      const protocolNode = state.rfNodes.find((n) => n.id === protocolId && n.type === 'protocol');
      if (!transportNode || !protocolNode) return;
      if (!state.rfEdges.some((edge) => edge.source === transportId && edge.target === protocolId)) return;
      const config = protocolNode?.data?.config as { kind?: string } | undefined;
      if (config?.kind === 'RawData') return;
      void state.sendWidgetValue(transportId, protocolId, binding, value);
      return;
    }
    case 'Manual': {
      const { transportId, template } = binding.params;
      if (template.trim() === '' || !state.rfNodes.some((n) => n.id === transportId && n.type === 'transport')) return;
      void state.sendText(transportId, template.replace(/\{value\}/g, String(value)));
      return;
    }
  }
}
