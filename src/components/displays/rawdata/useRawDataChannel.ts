// ============ RawData 通道选择 Hook ============
//
// 从 RawDataView 拆出 (视图文件 500 行上限): 通道选项派生 / 纯端口制选择解析 /
// 通道分类。数据订阅与渲染仍留在视图组件。

import { useCallback, useMemo } from 'react';
import { useAppStore } from '../../../store/appStore';
import { classifyRawDataChannel, resolveRawDataChannelKey } from '../../../lib/utils/rawDataChannel';
import { rawDataPortId } from '../../../lib/utils/nodeDef';

export interface RawDataChannelOption {
  key: string;
  sourceId: string;
  sourceHandle: string | undefined;
  /// 字节平面源 (Transport/Protocol) 的完整标签 (如 "Serial (a1b2)·rx") — 缺省回退 handle/widget 标签
  label?: string;
}

/// 通道选择 (单一事实源 = 控件配置 RawDataConfig.selectedInput) + 分类结果。
/// 返回的 rfEdges/rfNodes/widgets 供视图内发送上溯/标签解析复用, 避免重复订阅。
export function useRawDataChannel(widgetId?: string) {
  const rfEdges = useAppStore((s) => s.rfEdges);
  const rfNodes = useAppStore((s) => s.rfNodes);
  const widgets = useAppStore((s) => s.widgets);
  const updateWidget = useAppStore((s) => s.updateWidget);

  // 通道选择: 该 widget 的入边 (source, sourceHandle) 组合 (去重)
  // 字节平面源 (Transport/Protocol 全局节点) 附带节点标签, 避免多接口时选项同为 "rx"/"out" 无法区分
  const channelOptions = useMemo(() => {
    if (!widgetId) return [];
    const globalLabel = (id: string): string | null => {
      const n = rfNodes.find((n) => n.id === id);
      const cfg = (n?.data as { config?: { kind?: string } } | undefined)?.config;
      if (n?.type === 'transport') return `${cfg?.kind ?? '?'} (${id.slice(-4)})`;
      if (n?.type === 'protocol') return cfg?.kind ?? 'Protocol';
      return null;
    };
    const seen = new Set<string>();
    const options: RawDataChannelOption[] = [];
    for (const e of rfEdges) {
      if (e.target !== widgetId) continue;
      const sourceHandle = e.sourceHandle ?? undefined;
      const key = rawDataPortId(e.source, sourceHandle);
      if (seen.has(key)) continue;
      seen.add(key);
      const g = globalLabel(e.source);
      options.push({
        key,
        sourceId: e.source,
        sourceHandle,
        label: g ? `${g}·${sourceHandle ?? 'data'}` : undefined,
      });
    }
    return options;
  }, [widgetId, rfEdges, rfNodes]);

  // 纯端口制通道选择 (单一事实源 = 控件配置 RawDataConfig.selectedInput):
  // 配置选中且该连线仍存在 → 用它; 否则回退第一个已连接端口; 无连线 → '' (空态)。
  // 切换选择经 onChannelChange 写回配置, 触发操作历史与图同步 (Sink 节点无参数变化, 重编译无害)
  const ownWidget = useMemo(() => {
    const w = widgets.find((w) => w.kind === 'RawData' && w.params.id === widgetId);
    return w?.kind === 'RawData' ? w : undefined;
  }, [widgets, widgetId]);
  const channel = useMemo(
    () => resolveRawDataChannelKey(ownWidget?.params.selectedInput, channelOptions) ?? '',
    [channelOptions, ownWidget]
  );

  const onChannelChange = useCallback(
    (key: string) => {
      if (!ownWidget) return;
      updateWidget(ownWidget.params.id, {
        kind: 'RawData',
        params: { ...ownWidget.params, selectedInput: key },
      });
    },
    [ownWidget, updateWidget]
  );

  const selectedChannel = channelOptions.find((o) => o.key === channel);
  // 通道分类: FrameDecoder raw 口 = 节点旁路字节流; Transport/Protocol 源 = 接口原始字节流;
  // 控件字符串域输出口 = 字符串平面历史; 其余 = 数值流 (graphOutputs)
  const channelInfo = useMemo(
    () =>
      selectedChannel
        ? classifyRawDataChannel(selectedChannel, rfNodes, rfEdges, widgets)
        : null,
    [selectedChannel, rfNodes, rfEdges, widgets]
  );

  const sourceLabel = useCallback(
    (id: string) => {
      const w = widgets.find((w) => w.params.id === id);
      return w && 'label' in w.params ? w.params.label : id;
    },
    [widgets]
  );

  return {
    rfEdges,
    rfNodes,
    widgets,
    channelOptions,
    channel,
    selectedChannel,
    channelInfo,
    onChannelChange,
    sourceLabel,
  };
}
