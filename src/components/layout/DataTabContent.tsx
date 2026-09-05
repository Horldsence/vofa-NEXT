import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { WaveformChart } from '../displays/waveform/WaveformChart';
import { RawDataView } from '../displays/rawdata/RawDataView';
import { PieChart as PieChartWidget } from '../widgets/pieChart/PieChartWidget';
import { ImageViewer as ImageWidget } from '../widgets/image/ImageWidget';
import { SpectrumChart } from '../displays/widgets/SpectrumChart';
import { CommandSender } from '../displays/command/CommandSender';
import { CanView } from '../displays/can/CanView';
import { LogicView } from '../displays/logic/LogicView';
import { CompileErrorsView } from '../displays/compileErrors/CompileErrorsView';
import { CompileResultsView } from '../displays/compileResults/CompileResultsView';
import { OperationHistoryView } from '../displays/history/OperationHistoryView';
import { NodePropertiesPanel } from '../nodes/NodePropertiesPanel';
import { FrameDecoder } from '../displays/decoder/FrameDecoder';
import { Trigger } from '../widgets/trigger/TriggerView';
import { TableView } from '../displays/widgets/TableView';
import { AxisSettings } from '../displays/waveform/AxisSettings';
import { SuspenseFallback } from '../ui/SuspenseFallback';
import { lazy, Suspense, memo, useCallback, useEffect, useMemo } from 'react';
import type { WidgetConfig, ScopeAxisConfig, ProtocolConfig, LoopbackResult } from '../../types';
import { formatTimeBase, timeBaseToWindowMs } from '../../types';
import { waveformWindow, type WaveformWindowCache } from '../../lib/buffers/dataBuffer';
import { api } from '../../lib/tauri/tauri';
import { computeConnectedInputs, type ConnectedInput } from '../displays/waveform/waveformSeries';
import { useWaveformScopeStore, createPerWidgetState, type MeasurementsBundle } from '../../store/waveformScopeStore';
import { useWaveformSourceBuffer } from '../../lib/hooks/useWaveformSourceBuffer';
import { useWaveformMeasurements } from '../../lib/hooks/useWaveformMeasurements';
import { setWaveformOverviewActive } from '../../lib/buffers/sourceManagers';
import { traceProtocolSource } from '../../store/appStoreHelpers';

// 重型 3D 控件 (Three.js) — 懒加载, 首次切到 model3d Tab 时才拉取
const Model3DWidget = lazy(() => import('../widgets/model3d/Model3DWidget.lazy'));

// =====================================================================
// 各 Tab 类型分支 — 全部 memo 化, 且只接收稳定 props (模块级常量回调 /
// store 中稳定引用的 widget 对象), 使 DataTabContent 自身的重渲染
// (lang / widgets / rfEdges 等 store 订阅变化) 不会级联进重型子视图
// =====================================================================

interface WaveformTabViewProps {
  widget: Extract<WidgetConfig, { kind: 'Waveform' }>;
  axisConfig: ScopeAxisConfig;
  measurementBundle: MeasurementsBundle | null;
  measureChannel: number | null;
  autosetWarning: string | null;
  channelCount: number;
  buffer: WaveformWindowCache;
  sourceId: string | null;
  onConfigChange: (next: ScopeAxisConfig) => void;
  onAutoSet: () => void;
  onMeasureChannel: (channel: number | null) => void;
}

/// 波形分支 — 主图 + AxisSettings 侧栏
const WaveformTabView = memo(function WaveformTabView({
  widget,
  axisConfig,
  measurementBundle,
  measureChannel,
  autosetWarning,
  channelCount,
  buffer,
  sourceId,
  onConfigChange,
  onAutoSet,
  onMeasureChannel,
}: WaveformTabViewProps) {
  return (
    <div className="flex h-full w-full">
      <div className="flex-1 min-w-0 relative">
        <WaveformChart
          widget={widget}
          axisConfig={axisConfig}
          onConfigChange={onConfigChange}
          buffer={buffer}
          sourceId={sourceId}
        />
      </div>
      <div className="w-[256px] shrink-0 border-l border-border bg-bg-sidebar overflow-y-auto overflow-x-hidden">
        <AxisSettings
          config={axisConfig}
          onChange={onConfigChange}
          channelCount={channelCount}
          measurementBundle={measurementBundle}
          measureChannel={measureChannel}
          autosetWarning={autosetWarning}
          onAutoSet={onAutoSet}
          onMeasureChannel={onMeasureChannel}
        />
      </div>
    </div>
  );
});

interface RawTabViewProps {
  widgetId?: string;
}

const RawTabView = memo(function RawTabView({ widgetId }: RawTabViewProps) {
  return <RawDataView widgetId={widgetId} />;
});

interface PieTabViewProps {
  widget: Extract<WidgetConfig, { kind: 'PieChart' }>;
}

const PieTabView = memo(function PieTabView({ widget }: PieTabViewProps) {
  return (
    <div className="flex h-full p-2">
      <PieChartWidget widget={widget} full />
    </div>
  );
});

interface ImageTabViewProps {
  widget: Extract<WidgetConfig, { kind: 'Image' }>;
}

const ImageTabView = memo(function ImageTabView({ widget }: ImageTabViewProps) {
  return (
    <div className="flex h-full p-2">
      <ImageWidget widget={widget} full />
    </div>
  );
});

interface Model3DTabViewProps {
  widget: Extract<WidgetConfig, { kind: 'Model3D' }>;
}

const Model3DTabView = memo(function Model3DTabView({ widget }: Model3DTabViewProps) {
  return (
    <div className="flex h-full">
      <Suspense fallback={<SuspenseFallback />}>
        <Model3DWidget widget={widget} />
      </Suspense>
    </div>
  );
});

interface SpectrumTabViewProps {
  widget: Extract<WidgetConfig, { kind: 'Spectrum' }>;
}

const SpectrumTabView = memo(function SpectrumTabView({ widget }: SpectrumTabViewProps) {
  return (
    <div className="flex h-full">
      <SpectrumChart widget={widget} />
    </div>
  );
});

interface CommandTabViewProps {
  widget: Extract<WidgetConfig, { kind: 'Command' }>;
}

const CommandTabView = memo(function CommandTabView({ widget }: CommandTabViewProps) {
  return (
    <div className="flex h-full p-2">
      <CommandSender widget={widget} />
    </div>
  );
});

interface TableTabViewProps {
  widget: Extract<WidgetConfig, { kind: 'TableView' }>;
  loopbackHistory: LoopbackResult[] | undefined;
}

const TableTabView = memo(function TableTabView({ widget, loopbackHistory }: TableTabViewProps) {
  return (
    <div className="flex h-full w-full">
      <TableView widget={widget} loopbackHistory={loopbackHistory} />
    </div>
  );
});

interface FrameDecoderTabViewProps {
  widget: Extract<WidgetConfig, { kind: 'FrameDecoder' }>;
}

const FrameDecoderTabView = memo(function FrameDecoderTabView({ widget }: FrameDecoderTabViewProps) {
  return (
    <div className="flex h-full w-full">
      <FrameDecoder widget={widget} />
    </div>
  );
});

interface TriggerTabViewProps {
  widget: Extract<WidgetConfig, { kind: 'Trigger' }>;
}

const TriggerTabView = memo(function TriggerTabView({ widget }: TriggerTabViewProps) {
  return (
    <div className="flex h-full w-full">
      <Trigger widget={widget} />
    </div>
  );
});

/// 无 props 分支 — 模块级元素常量, 每次渲染返回同一引用, React 在 beginWork 中
/// 因 props 引用相等直接 bailout, 完全跳过子树重渲染
const canTabContent = (
  <div className="flex h-full w-full">
    <CanView />
  </div>
);
const logicTabContent = (
  <div className="flex h-full w-full">
    <LogicView />
  </div>
);
const compileErrorsTabContent = (
  <div className="flex h-full w-full">
    <CompileErrorsView />
  </div>
);
const compileResultsTabContent = (
  <div className="flex h-full w-full">
    <CompileResultsView />
  </div>
);
const operationHistoryTabContent = (
  <div className="flex h-full w-full">
    <OperationHistoryView />
  </div>
);
const nodePropertiesTabContent = (
  <div className="flex h-full w-full">
    <NodePropertiesPanel />
  </div>
);

/// 单个数据 Tab 的内容渲染器 — 由 DockCardFrame 挂载, 可被多个卡片各自实例化
/// 波形 Tab 的 axisConfig / measurements 按 widgetId 存于 waveformScopeStore,
/// Tab 在卡片间移动或拆分为独立面板时配置不丢失
export const DataTabContent = memo(function DataTabContent({ tabId }: { tabId: string }) {
  const lang = useAppStore((s) => s.lang);
  const dataTabs = useAppStore((s) => s.dataTabs);
  const widgets = useAppStore((s) => s.widgets);
  const rfEdges = useAppStore((s) => s.rfEdges);
  const rfNodes = useAppStore((s) => s.rfNodes);
  // 主 Protocol 节点 (第一个) 的配置与检测通道数 — 固定波形 Tab 的通道数依据
  const primaryProtocolId = useMemo(
    () => rfNodes.find((n) => n.type === 'protocol' && n.data?.global === true)?.id ?? null,
    [rfNodes]
  );
  const primaryProtocolConfig = useMemo(() => {
    const n = rfNodes.find((x) => x.id === primaryProtocolId);
    return n ? ((n.data as { config?: ProtocolConfig }).config ?? null) : null;
  }, [rfNodes, primaryProtocolId]);
  const detectedChannels = useAppStore((s) =>
    primaryProtocolId ? (s.detectedChannels[primaryProtocolId] ?? null) : null
  );
  // 不订阅 rawDataVersion: channel_count 仅在协议/检测变化时改变
  const winChannelCount = waveformWindow.get().channel_count;

  const tab = dataTabs.find((t) => t.id === tabId);
  const isWaveformTab = tab?.type === 'waveform' || tab?.type === 'waveform-extra';

  // 计算默认波形的通道数: 自动模式优先用检测到的通道数, 其次用窗口缓存, 最后兜底 4
  const defaultChannelCount = useMemo(() => {
    if (!primaryProtocolConfig) return winChannelCount || 4;
    if (primaryProtocolConfig.kind === 'RawData' || primaryProtocolConfig.kind === 'Slcan' || primaryProtocolConfig.kind === 'CandleLight' || primaryProtocolConfig.kind === 'LogicDecode') {
      return 4;
    }
    return (primaryProtocolConfig.channels ?? detectedChannels ?? (winChannelCount || 4));
  }, [primaryProtocolConfig, detectedChannels, winChannelCount]);

  // 默认波形控件（固定 Tab 使用）
  const defaultWaveformWidget: Extract<WidgetConfig, { kind: 'Waveform' }> = useMemo(
    () => ({
      kind: 'Waveform',
      params: {
        id: 'default-waveform',
        label: 'Waveform',
        channels: defaultChannelCount,
        max_points: 10000,
        visible_channels: Array.from({ length: defaultChannelCount }, () => true),
      },
    }),
    [defaultChannelCount]
  );

  const waveWidget =
    (isWaveformTab && tab?.widgetId
      ? (widgets.find(
          (w) => w.params.id === tab.widgetId && w.kind === 'Waveform'
        ) as Extract<WidgetConfig, { kind: 'Waveform' }> | undefined)
      : undefined) ?? defaultWaveformWidget;
  const wid = waveWidget.params.id;
  const channelCount = waveWidget.params.channels;

  // 波形数据源: 固定 Tab = 主 Protocol 节点; 控件波形 = 输入边向上溯源到的 Protocol 节点
  // (无连接时 sourceId 为 null → 空缓冲, 不订阅)
  const waveSourceId = useMemo(() => {
    if (!isWaveformTab) return null;
    if (wid === 'default-waveform') return primaryProtocolId;
    return traceProtocolSource(wid, rfEdges, rfNodes);
  }, [isWaveformTab, wid, primaryProtocolId, rfEdges, rfNodes]);
  const waveBuffer = useWaveformSourceBuffer(waveSourceId);

  const ensureWidget = useWaveformScopeStore((s) => s.ensureWidget);
  const setConfig = useWaveformScopeStore((s) => s.setConfig);
  const setMeasureChannel = useWaveformScopeStore((s) => s.setMeasureChannel);
  const setAutosetWarning = useWaveformScopeStore((s) => s.setAutosetWarning);
  const pruneWidgets = useWaveformScopeStore((s) => s.pruneWidgets);
  const widgetState = useWaveformScopeStore((s) => s.states[wid]);

  // 波形 state 兜底 — memo 保持引用稳定, 避免每次渲染新建对象击穿 WaveformTabView memo
  const fallbackState = useMemo(() => createPerWidgetState(channelCount), [channelCount]);
  const widgetScopeState = widgetState ?? fallbackState;

  // 波形图的派生序列 (MATH/Filter 接入) — 与图上 series 同一语义, 参与
  // 后端测量与 AutoSet 周期检测 (慢派生波形必须驱动时基)
  const derivedSelectors = useMemo(() => {
    if (!isWaveformTab || wid === 'default-waveform') return [];
    return computeConnectedInputs(wid, channelCount, rfEdges)
      .filter((i): i is Extract<ConnectedInput, { kind: 'derived' }> => i.kind === 'derived')
      .map((i) => ({
        sink_id: wid,
        source_id: i.sourceId,
        source_handle: i.sourceHandle,
      }));
  }, [isWaveformTab, wid, channelCount, rfEdges]);
  const derivedSelectorsKey = JSON.stringify(derivedSelectors);

  // 后端测量流 — 统计/周期在权威缓冲上计算, 随时基窗口重订阅。
  // 测量与测试数据源彻底解耦: 只依赖 (sourceId, 当前时基窗口)。
  useWaveformMeasurements({
    sourceId: waveSourceId,
    windowMs: timeBaseToWindowMs(widgetScopeState.config.timeBase),
    derivedSelectors,
    widgetId: wid,
    channelCount,
  });

  // 波形停止时暂停概览推送 (后端不再空转全缓冲 min-max), 恢复运行时重订阅
  const waveRunning = (widgetState ?? fallbackState).config.running;
  useEffect(() => {
    if (waveSourceId) setWaveformOverviewActive(waveSourceId, waveRunning);
  }, [waveSourceId, waveRunning]);

  // 懒初始化 + 通道数扩展
  useEffect(() => {
    if (isWaveformTab) ensureWidget(wid, channelCount);
  }, [isWaveformTab, wid, channelCount, ensureWidget]);

  // 移除 widget 时清理其配置
  useEffect(() => {
    pruneWidgets(widgets.map((w) => w.params.id));
  }, [widgets, pruneWidgets]);

  /// 稳定回调 — 仅在 widget 身份 / 连线变化时重建, 保证 memo 分支在无关重渲染时跳过
  const handleConfigChange = useCallback(
    (next: ScopeAxisConfig) => setConfig(wid, channelCount, next),
    [wid, channelCount, setConfig]
  );

  const handleMeasureChannel = useCallback(
    (channel: number | null) => setMeasureChannel(wid, channelCount, channel),
    [wid, channelCount, setMeasureChannel]
  );

  /// AutoSet — 后端周期检测 + 1-2-5 拟合 (原始层/金字塔快照), 前端仅合并建议。
  /// 时间轴: 检测周期 × 4 个周期; 不可测时回退最近数据窗口拟合。
  const handleAutoSet = useCallback(() => {
    if (!waveSourceId) return;
    // 读最新 config (不经 selector 依赖), 避免测量更新导致回调重建
    const curConfig =
      useWaveformScopeStore.getState().states[wid]?.config ??
      createPerWidgetState(channelCount).config;
    const currentVPerDiv = curConfig.channels.map((c) => c.vPerDiv);
    const derived = JSON.parse(derivedSelectorsKey) as typeof derivedSelectors;
    void api
      .computeWaveformAutoset(waveSourceId, [], derived, curConfig.sharedY, currentVPerDiv)
      .then((suggestion) => {
        const nextChannels = curConfig.channels.slice();
        suggestion.channels.forEach((fit, idx) => {
          while (nextChannels.length <= idx) {
            nextChannels.push({ vPerDiv: 1, position: 0, show: true, coupling: 'DC' });
          }
          nextChannels[idx] = {
            ...nextChannels[idx],
            vPerDiv: fit.v_per_div,
            position: fit.position,
          };
        });
        setConfig(wid, channelCount, {
          ...curConfig,
          timeBase: suggestion.time_base_sec,
          channels: nextChannels,
          hPosition: suggestion.h_position,
          running: suggestion.running,
        });
        // 钳位/压扁风险提示 (一次性, 显示在面板顶部)
        let warning: string | null = null;
        if (suggestion.clamped) {
          warning = t(lang, 'autosetClampedWarning')
            .replace('{actual}', formatTimeBase(suggestion.time_base_sec))
            .replace('{requested}', formatTimeBase(suggestion.requested_window_sec / 10));
        } else if (suggestion.shared_y_span_risk) {
          warning = t(lang, 'autosetSpanRiskWarning');
        }
        setAutosetWarning(wid, channelCount, warning);
      })
      .catch((e: unknown) => console.error('自动设置失败:', e));
  }, [wid, channelCount, waveSourceId, derivedSelectorsKey, setConfig, setAutosetWarning, lang]);

  if (!tab) return null;

  const noWidget = (
    <div className="flex items-center justify-center h-full text-text-secondary text-sm">
      {t(lang, 'noWidgets')}
    </div>
  );

  switch (tab.type) {
    case 'waveform':
    case 'waveform-extra': {
      const st = widgetScopeState;
      return (
        <WaveformTabView
          widget={waveWidget}
          axisConfig={st.config}
          measurementBundle={st.measurements}
          measureChannel={st.measureChannel}
          autosetWarning={st.autosetWarning}
          channelCount={channelCount}
          buffer={waveBuffer}
          sourceId={waveSourceId}
          onConfigChange={handleConfigChange}
          onAutoSet={handleAutoSet}
          onMeasureChannel={handleMeasureChannel}
        />
      );
    }
    case 'raw':
      return <RawTabView widgetId={tab.widgetId} />;
    case 'pie': {
      const widget = widgets.find(
        (w) => w.params.id === tab.widgetId && w.kind === 'PieChart'
      ) as Extract<WidgetConfig, { kind: 'PieChart' }> | undefined;
      if (!widget) return noWidget;
      return <PieTabView widget={widget} />;
    }
    case 'image': {
      const widget = widgets.find(
        (w) => w.params.id === tab.widgetId && w.kind === 'Image'
      ) as Extract<WidgetConfig, { kind: 'Image' }> | undefined;
      if (!widget) return noWidget;
      return <ImageTabView widget={widget} />;
    }
    case 'model3d': {
      const widget = widgets.find(
        (w) => w.params.id === tab.widgetId && w.kind === 'Model3D'
      ) as Extract<WidgetConfig, { kind: 'Model3D' }> | undefined;
      if (!widget) return noWidget;
      return <Model3DTabView widget={widget} />;
    }
    case 'spectrum': {
      const widget = widgets.find(
        (w) => w.params.id === tab.widgetId && w.kind === 'Spectrum'
      ) as Extract<WidgetConfig, { kind: 'Spectrum' }> | undefined;
      if (!widget) return noWidget;
      return <SpectrumTabView widget={widget} />;
    }
    case 'command': {
      const widget = widgets.find(
        (w) => w.params.id === tab.widgetId && w.kind === 'Command'
      ) as Extract<WidgetConfig, { kind: 'Command' }> | undefined;
      if (!widget) return noWidget;
      return <CommandTabView widget={widget} />;
    }
    case 'can':
      return canTabContent;
    case 'logic':
      return logicTabContent;
    case 'compile-errors':
      return compileErrorsTabContent;
    case 'compile-results':
      return compileResultsTabContent;
    case 'operation-history':
      return operationHistoryTabContent;
    case 'node-properties':
      return nodePropertiesTabContent;
    case 'table-view': {
      const widget = widgets.find(
        (w) => w.params.id === tab.widgetId && w.kind === 'TableView'
      ) as Extract<WidgetConfig, { kind: 'TableView' }> | undefined;
      if (!widget) return noWidget;
      const cmdWidget = widgets.find(
        (w) => w.kind === 'Command' && w.params.loopbackEnabled
      ) as Extract<WidgetConfig, { kind: 'Command' }> | undefined;
      return <TableTabView widget={widget} loopbackHistory={cmdWidget?.params.loopbackHistory} />;
    }
    case 'frame-decoder': {
      const widget = widgets.find(
        (w) => w.params.id === tab.widgetId && w.kind === 'FrameDecoder'
      ) as Extract<WidgetConfig, { kind: 'FrameDecoder' }> | undefined;
      if (!widget) return noWidget;
      return <FrameDecoderTabView widget={widget} />;
    }
    case 'trigger': {
      const widget = widgets.find(
        (w) => w.params.id === tab.widgetId && w.kind === 'Trigger'
      ) as Extract<WidgetConfig, { kind: 'Trigger' }> | undefined;
      if (!widget) return noWidget;
      return <TriggerTabView widget={widget} />;
    }
    default:
      return null;
  }
});

