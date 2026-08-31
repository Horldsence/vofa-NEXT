import { useAppStore } from '../../store/appStore';
import { useShallow } from 'zustand/react/shallow';
import { resolveStringSource } from '../utils/stringPorts';

/// 读取所有连到本 widget 的字符串输入端口值 (字符串平面版 useGraphInputs)
/// 返回 portId -> string 的映射; 无连接的端口取 fallback
///
/// 窄订阅: useShallow 逐端口比较, 仅本 widget 的任一字符串输入变化时才重渲染
export function useStringInputs(
  widgetId: string,
  portIds: string[],
  fallback = ''
): Record<string, string> {
  const edges = useAppStore((s) => s.rfEdges);

  // 每个 port 的上游 (source, handle); 无连接为 null
  const sources = portIds.map((portId) => resolveStringSource(edges, widgetId, portId));

  // 窄选择器: 按端口取上游字符串输出, useShallow 逐元素比较 (string|undefined)
  const values = useAppStore(
    useShallow((s) =>
      sources.map((src) => (src ? s.customTextOutputs[src.source]?.[src.handle] : undefined))
    )
  );

  const result: Record<string, string> = {};
  for (let i = 0; i < portIds.length; i++) {
    result[portIds[i]] = values[i] ?? fallback;
  }
  return result;
}
