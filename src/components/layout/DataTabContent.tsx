import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import {
  LineChart as LineChartIcon,
  Activity as ActivityIcon,
  PieChart as PieIcon,
  Image as ImageIcon,
  Box as BoxIcon,
  BarChart3 as BarChart3Icon,
  Send as SendIcon,
  Cpu as CpuIcon,
  CircuitBoard as CircuitBoardIcon,
  ScanText as ScanTextIcon,
  Zap as ZapIcon,
  AlertTriangle as AlertTriangleIcon,
  ListTree as ListTreeIcon,
  History as HistoryIcon,
} from 'lucide-react';
import { WaveformChart } from '../displays/waveform/WaveformChart';
import { RawDataView } from '../displays/rawdata/RawDataView';
import { PieChart } from '../displays/widgets/PieChart';
import { ImageViewer } from '../displays/widgets/ImageViewer';
import { SpectrumChart } from '../displays/widgets/SpectrumChart';
import { CommandSender } from '../displays/command/CommandSender';
import { CanView } from '../displays/can/CanView';
import { LogicView } from '../displays/logic/LogicView';
import { CompileErrorsView } from '../displays/compileErrors/CompileErrorsView';
import { CompileResultsView } from '../displays/compileResults/CompileResultsView';
import { OperationHistoryView } from '../displays/history/OperationHistoryView';
import { FrameDecoder } from '../displays/decoder/FrameDecoder';
import { Trigger } from '../controls/Trigger';
import { TableView } from '../displays/widgets/TableView';
import { AxisSettings } from '../displays/waveform/AxisSettings';
import { SuspenseFallback } from '../ui/SuspenseFallback';
import { lazy, Suspense, memo, useCallback, useEffect, useMemo } from 'react';
import type { WidgetConfig, ScopeMeasurements, ScopeAxisConfig, ProtocolConfig, LoopbackResult } from '../../types';
import { getEffectiveChannel } from '../../types';
import { waveformWindow, type WaveformWindowCache } from '../../lib/buffers/dataBuffer';
import { computeMeasurements, computeAutoSetConfig, applyCoupling } from '../../lib/utils/scopeUtils';
import { computeConnectedInputs, type ConnectedInput } from '../displays/waveform/waveformSeries';
import { useWaveformScopeStore, createPerWidgetState } from '../../store/waveformScopeStore';
import { useWaveformSourceBuffer } from '../../lib/hooks/useWaveformSourceBuffer';
import { traceProtocolSource } from '../../store/appStoreHelpers';

// 重型 3D 控件 (Three.js) — 懒加载, 首次切到 model3d Tab 时才拉取
const Model3DWidget = lazy(() => import('../displays/widgets/Model3DWidget.lazy'));

/// 稳定空回调 — DataPanel 展示控件不可删除; 共享引用让 memo 包装的控件跳过父级重渲染
const noopRemove = () => {};

// =====================================================================
// 各 Tab 类型分支 — 全部 memo 化, 且只接收稳定 props (模块级常量回调 /
// store 中稳定引用的 widget 对象), 使 DataTabContent 自身的重渲染
// (lang / widgets / rfEdges 等 store 订阅变化) 不会级联进重型子视图
// =====================================================================

interface WaveformTabViewProps {
  widget: Extract<WidgetConfig, { kind: 'Waveform' }>;
  axisConfig: ScopeAxisConfig;
  measurements: ScopeMeasurements | null;
  channelCount: number;
  buffer: WaveformWindowCache;
  onConfigChange: (next: ScopeAxisConfig) => void;
  onAutoSet: () => void;
}

/// 波形分支 — 主图 + AxisSettings 侧栏
const WaveformTabView = memo(function WaveformTabView({
  widget,
  axisConfig,
  measurements,
  channelCount,
  buffer,
  onConfigChange,
  onAutoSet,
}: WaveformTabViewProps) {
  return (
    <div className="flex h-full w-full">
      <div className="flex-1 min-w-0 relative">
        <WaveformChart widget={widget} axisConfig={axisConfig} onConfigChange={onConfigChange} buffer={buffer} />
      </div>
      <div className="w-[256px] shrink-0 border-l border-border bg-bg-sidebar overflow-y-auto overflow-x-hidden">
        <AxisSettings
          config={axisConfig}
          onChange={onConfigChange}
          channelCount={channelCount}
          measurements={measurements}
          onAutoSet={onAutoSet}
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
  onRemove: () => void;
}

const PieTabView = memo(function PieTabView({ widget, onRemove }: PieTabViewProps) {
  return (
    <div className="flex h-full p-2">
      <PieChart widget={widget} onRemove={onRemove} full />
    </div>
  );
});

interface ImageTabViewProps {
  widget: Extract<WidgetConfig, { kind: 'Image' }>;
  onRemove: () => void;
}

const ImageTabView = memo(function ImageTabView({ widget, onRemove }: ImageTabViewProps) {
  return (
    <div className="flex h-full p-2">
      <ImageViewer widget={widget} onRemove={onRemove} full />
    </div>
  );
});

interface Model3DTabViewProps {
  widget: Extract<WidgetConfig, { kind: 'Model3D' }>;
  onRemove: () => void;
}

const Model3DTabView = memo(function Model3DTabView({ widget, onRemove }: Model3DTabViewProps) {
  return (
    <div className="flex h-full">
      <Suspense fallback={<SuspenseFallback />}>
        <Model3DWidget widget={widget} onRemove={onRemove} />
      </Suspense>
    </div>
  );
});

interface SpectrumTabViewProps {
  widget: Extract<WidgetConfig, { kind: 'Spectrum' }>;
  onRemove: () => void;
}

const SpectrumTabView = memo(function SpectrumTabView({ widget, onRemove }: SpectrumTabViewProps) {
  return (
    <div className="flex h-full">
      <SpectrumChart widget={widget} onRemove={onRemove} />
    </div>
  );
});

interface CommandTabViewProps {
  widget: Extract<WidgetConfig, { kind: 'Command' }>;
  onRemove: () => void;
}

const CommandTabView = memo(function CommandTabView({ widget, onRemove }: CommandTabViewProps) {
  return (
    <div className="flex h-full p-2">
      <CommandSender widget={widget} onRemove={onRemove} />
    </div>
  );
});

interface TableTabViewProps {
  widget: Extract<WidgetConfig, { kind: 'TableView' }>;
  onRemove: () => void;
  loopbackHistory: LoopbackResult[] | undefined;
}

const TableTabView = memo(function TableTabView({ widget, onRemove, loopbackHistory }: TableTabViewProps) {
  return (
    <div className="flex h-full w-full">
      <TableView widget={widget} onRemove={onRemove} loopbackHistory={loopbackHistory} />
    </div>
  );
});

interface FrameDecoderTabViewProps {
  widget: Extract<WidgetConfig, { kind: 'FrameDecoder' }>;
  onRemove: () => void;
}

const FrameDecoderTabView = memo(function FrameDecoderTabView({ widget, onRemove }: FrameDecoderTabViewProps) {
  return (
    <div className="flex h-full w-full">
      <FrameDecoder widget={widget} onRemove={onRemove} />
    </div>
  );
});

interface TriggerTabViewProps {
  widget: Extract<WidgetConfig, { kind: 'Trigger' }>;
  onRemove: () => void;
}

const TriggerTabView = memo(function TriggerTabView({ widget, onRemove }: TriggerTabViewProps) {
  return (
    <div className="flex h-full w-full">
      <Trigger widget={widget} onRemove={onRemove} />
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
  const setMeasurements = useWaveformScopeStore((s) => s.setMeasurements);
  const pruneWidgets = useWaveformScopeStore((s) => s.pruneWidgets);
  const widgetState = useWaveformScopeStore((s) => s.states[wid]);

  // 波形 state 兜底 — memo 保持引用稳定, 避免每次渲染新建对象击穿 WaveformTabView memo
  const fallbackState = useMemo(() => createPerWidgetState(channelCount), [channelCount]);

  // 懒初始化 + 通道数扩展
  useEffect(() => {
    if (isWaveformTab) ensureWidget(wid, channelCount);
  }, [isWaveformTab, wid, channelCount, ensureWidget]);

  // 移除 widget 时清理其配置
  useEffect(() => {
    pruneWidgets(widgets.map((w) => w.params.id));
  }, [widgets, pruneWidgets]);

  // 测量值计算 — rAF 循环, 波形数据版本变化时更新 (基于第一可见通道, 与主图显示一致)
  const running = isWaveformTab && (widgetState?.config.running ?? true);
  useEffect(() => {
    if (!running) return;
    let raf = 0;
    const loop = () => {
      const version = waveBuffer.version;
      const cur = useWaveformScopeStore.getState().states[wid];
      if (cur && version !== cur.lastMeasureVersion) {
        const win = waveBuffer.get();
        let m: ScopeMeasurements | null = null;
        if (win.timestamps.length >= 2) {
          const chIdx = cur.config.channels.findIndex((c) => c.show);
          const targetIdx = chIdx >= 0 ? chIdx : 0;
          const ch = win.channels[targetIdx];
          if (ch && ch.length > 0) {
            const eff = getEffectiveChannel(cur.config, targetIdx);
            const coupled = applyCoupling(ch, eff.coupling);
            m = computeMeasurements(coupled, win.timestamps);
          }
        }
        setMeasurements(wid, channelCount, version, m);
      }
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  }, [running, wid, channelCount, setMeasurements, waveBuffer]);

  /// 稳定回调 — 仅在 widget 身份 / 连线变化时重建, 保证 memo 分支在无关重渲染时跳过
  const handleConfigChange = useCallback(
    (next: ScopeAxisConfig) => setConfig(wid, channelCount, next),
    [wid, channelCount, setConfig]
  );

  const handleAutoSet = useCallback(() => {
    const win = waveBuffer.get();
    // 与主图/缩略图共用 computeConnectedInputs, 避免 "空则全通道" 回退分叉
    const connected =
      wid === 'default-waveform'
        ? Array.from({ length: win.channel_count || channelCount }, (_, i) => i)
        : computeConnectedInputs(wid, channelCount, rfEdges)
            .filter((i): i is Extract<ConnectedInput, { kind: 'channel' }> => i.kind === 'channel')
            .map((i) => i.idx);
    // 读最新 config (不经 selector 依赖), 避免测量更新导致回调重建
    const curConfig = useWaveformScopeStore.getState().states[wid]?.config ?? createPerWidgetState(channelCount).config;
    const autoNext = computeAutoSetConfig(win, curConfig, connected);
    setConfig(wid, channelCount, autoNext);
  }, [wid, channelCount, rfEdges, setConfig, waveBuffer]);

  if (!tab) return null;

  const noWidget = (
    <div className="flex items-center justify-center h-full text-text-secondary text-sm">
      {t(lang, 'noWidgets')}
    </div>
  );

  switch (tab.type) {
    case 'waveform':
    case 'waveform-extra': {
      const st = widgetState ?? fallbackState;
      return (
        <WaveformTabView
          widget={waveWidget}
          axisConfig={st.config}
          measurements={st.measurements}
          channelCount={channelCount}
          buffer={waveBuffer}
          onConfigChange={handleConfigChange}
          onAutoSet={handleAutoSet}
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
      return <PieTabView widget={widget} onRemove={noopRemove} />;
    }
    case 'image': {
      const widget = widgets.find(
        (w) => w.params.id === tab.widgetId && w.kind === 'Image'
      ) as Extract<WidgetConfig, { kind: 'Image' }> | undefined;
      if (!widget) return noWidget;
      return <ImageTabView widget={widget} onRemove={noopRemove} />;
    }
    case 'model3d': {
      const widget = widgets.find(
        (w) => w.params.id === tab.widgetId && w.kind === 'Model3D'
      ) as Extract<WidgetConfig, { kind: 'Model3D' }> | undefined;
      if (!widget) return noWidget;
      return <Model3DTabView widget={widget} onRemove={noopRemove} />;
    }
    case 'spectrum': {
      const widget = widgets.find(
        (w) => w.params.id === tab.widgetId && w.kind === 'Spectrum'
      ) as Extract<WidgetConfig, { kind: 'Spectrum' }> | undefined;
      if (!widget) return noWidget;
      return <SpectrumTabView widget={widget} onRemove={noopRemove} />;
    }
    case 'command': {
      const widget = widgets.find(
        (w) => w.params.id === tab.widgetId && w.kind === 'Command'
      ) as Extract<WidgetConfig, { kind: 'Command' }> | undefined;
      if (!widget) return noWidget;
      return <CommandTabView widget={widget} onRemove={noopRemove} />;
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
    case 'table-view': {
      const widget = widgets.find(
        (w) => w.params.id === tab.widgetId && w.kind === 'TableView'
      ) as Extract<WidgetConfig, { kind: 'TableView' }> | undefined;
      if (!widget) return noWidget;
      const cmdWidget = widgets.find(
        (w) => w.kind === 'Command' && w.params.loopbackEnabled
      ) as Extract<WidgetConfig, { kind: 'Command' }> | undefined;
      return <TableTabView widget={widget} onRemove={noopRemove} loopbackHistory={cmdWidget?.params.loopbackHistory} />;
    }
    case 'frame-decoder': {
      const widget = widgets.find(
        (w) => w.params.id === tab.widgetId && w.kind === 'FrameDecoder'
      ) as Extract<WidgetConfig, { kind: 'FrameDecoder' }> | undefined;
      if (!widget) return noWidget;
      return <FrameDecoderTabView widget={widget} onRemove={noopRemove} />;
    }
    case 'trigger': {
      const widget = widgets.find(
        (w) => w.params.id === tab.widgetId && w.kind === 'Trigger'
      ) as Extract<WidgetConfig, { kind: 'Trigger' }> | undefined;
      if (!widget) return noWidget;
      return <TriggerTabView widget={widget} onRemove={noopRemove} />;
    }
    default:
      return null;
  }
});

/// 数据 Tab 图标 (按类型)
export function DataTabIcon({ type, size = 12 }: { type: string; size?: number }) {
  switch (type) {
    case 'waveform':
    case 'waveform-extra':
      return <LineChartIcon size={size} />;
    case 'raw':
      return <ActivityIcon size={size} />;
    case 'pie':
      return <PieIcon size={size} />;
    case 'image':
      return <ImageIcon size={size} />;
    case 'model3d':
      return <BoxIcon size={size} />;
    case 'spectrum':
      return <BarChart3Icon size={size} />;
    case 'command':
      return <SendIcon size={size} />;
    case 'can':
      return <CpuIcon size={size} />;
    case 'logic':
      return <CircuitBoardIcon size={size} />;
    case 'frame-decoder':
      return <ScanTextIcon size={size} />;
    case 'trigger':
      return <ZapIcon size={size} />;
    case 'table-view':
      return <BarChart3Icon size={size} />;
    case 'compile-errors':
      return <AlertTriangleIcon size={size} />;
    case 'compile-results':
      return <ListTreeIcon size={size} />;
    case 'operation-history':
      return <HistoryIcon size={size} />;
    default:
      return null;
  }
}
