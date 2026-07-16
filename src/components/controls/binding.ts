import type { WidgetBinding } from '../../types';
import { useAppStore } from '../../store/appStore';

/// 根据绑定模式发送控件值
/// - None: 不发送
/// - Auto: 调用后端 encode_channel(channel, value)
/// - Manual: 使用模板 {value} 替换后以字符串发送
export function sendBindingValue(binding: WidgetBinding, value: number) {
  const sendWidgetValue = useAppStore.getState().sendWidgetValue;
  const sendText = useAppStore.getState().sendText;
  const protocolConfig = useAppStore.getState().protocolConfig;

  switch (binding.mode) {
    case 'None':
      return;
    case 'Auto':
      if (protocolConfig.kind === 'RawData') return;
      sendWidgetValue(binding, value);
      return;
    case 'Manual':
      sendText(binding.params.template.replace(/\{value\}/g, String(value)));
      return;
  }
}
