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
} from 'lucide-react';
import { WaveformChart } from '../displays/waveform/WaveformChart';
import { RawDataView } from '../displays/rawdata/RawDataView';
import { PieChart } from '../displays/widgets/PieChart';
import { ImageViewer } from '../displays/widgets/ImageViewer';
import { SpectrumChart } from '../displays/widgets/SpectrumChart';
import { CommandSender } from '../displays/command/CommandSender';
import { CanView } from '../displays/can/CanView';
import { LogicView } from '../displays/logic/LogicView';
import { FrameDecoder } from '../displays/decoder/FrameDecoder';
import { TableView } from '../displays/widgets/TableView';
import { AxisSettings } from '../displays/waveform/AxisSettings';
import { SuspenseFallback } from '../ui/SuspenseFallback';
import { lazy, Suspense, useEffect, useMemo } from 'react';
import type { WidgetConfig, ScopeMeasurements, ProtocolConfig } from '../../types';
import { getEffectiveChannel } from '../../types';
import { waveformWindow } from '../../lib/buffers/dataBuffer';
import { computeMeasurements, computeAutoSetConfig, applyCoupling } from '../../lib/utils/scopeUtils';
import { computeConnectedInputs, type ConnectedInput } from '../displays/waveform/waveformSeries';
import { useWaveformScopeStore, createPerWidgetState } from '../../store/waveformScopeStore';

// 重型 3D 控件 (Three.js) — 懒加载, 首次切到 model3d Tab 时才拉取
const Model3DWidget = lazy(() => import('../displays/widgets/Model3DWidget.lazy'));

/// 稳定空回调 — DataPanel 展示控件不可删除; 共享引用让 memo 包装的控件跳过父级重渲染
const noopRemove = () => {};

/// 单个数据 Tab 的内容渲染器 — 由 DockCardFrame 挂载, 可被多个卡片各自实例化
/// 波形 Tab 的 axisConfig / measurements 按 widgetId 存于 waveformScopeStore,
/// Tab 在卡片间移动或拆分为独立面板时配置不丢失
export function DataTabContent({ tabId }: { tabId: string }) {
  const lang = useAppStore((s) => s.lang);
  const dataTabs = useAppStore((s) => s.dataTabs);
  const widgets = useAppStore((s) => s.widgets);
  const rfEdges = useAppStore((s) => s.rfEdges);
  const protocolConfig = useAppStore((s) => s.protocolConfig);
  const detectedChannels = useAppStore((s) => s.detectedChannels);
  // 不订阅 rawDataVersion: channel_count 仅在协议/检测变化时改变
  const winChannelCount = waveformWindow.get().channel_count;

  const tab = dataTabs.find((t) => t.id === tabId);
  const isWaveformTab = tab?.type === 'waveform' || tab?.type === 'waveform-extra';

  // 计算默认波形的通道数: 自动模式优先用检测到的通道数, 其次用窗口缓存, 最后兜底 4
  const defaultChannelCount = useMemo(() => {
    if (protocolConfig.kind === 'RawData' || protocolConfig.kind === 'Slcan' || protocolConfig.kind === 'CandleLight' || protocolConfig.kind === 'LogicDecode') {
      return 4;
    }
    return ((protocolConfig as Extract<ProtocolConfig, { channels?: number | null }>).channels ?? detectedChannels ?? (winChannelCount || 4));
  }, [protocolConfig, detectedChannels, winChannelCount]);

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

  const ensureWidget = useWaveformScopeStore((s) => s.ensureWidget);
  const setConfig = useWaveformScopeStore((s) => s.setConfig);
  const setMeasurements = useWaveformScopeStore((s) => s.setMeasurements);
  const pruneWidgets = useWaveformScopeStore((s) => s.pruneWidgets);
  const widgetState = useWaveformScopeStore((s) => s.states[wid]);

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
      const version = waveformWindow.version;
      const cur = useWaveformScopeStore.getState().states[wid];
      if (cur && version !== cur.lastMeasureVersion) {
        const win = waveformWindow.get();
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
  }, [running, wid, channelCount, setMeasurements]);

  if (!tab) return null;

  const noWidget = (
    <div className="flex items-center justify-center h-full text-text-secondary text-sm">
      {t(lang, 'noWidgets')}
    </div>
  );

  switch (tab.type) {
    case 'waveform':
    case 'waveform-extra': {
      const st = widgetState ?? createPerWidgetState(channelCount);
      return (
        <div className="flex h-full w-full">
          <div className="flex-1 min-w-0 relative">
            <WaveformChart
              widget={waveWidget}
              axisConfig={st.config}
              onConfigChange={(next) => setConfig(wid, channelCount, next)}
            />
          </div>
          <div className="w-[256px] flex-shrink-0 border-l border-border bg-bg-sidebar overflow-y-auto overflow-x-hidden">
            <AxisSettings
              config={st.config}
              onChange={(next) => setConfig(wid, channelCount, next)}
              channelCount={channelCount}
              measurements={st.measurements}
              onAutoSet={() => {
                const win = waveformWindow.get();
                // 与主图/缩略图共用 computeConnectedInputs, 避免 "空则全通道" 回退分叉
                const connected =
                  wid === 'default-waveform'
                    ? Array.from({ length: win.channel_count || channelCount }, (_, i) => i)
                    : computeConnectedInputs(wid, channelCount, rfEdges)
                        .filter((i): i is Extract<ConnectedInput, { kind: 'channel' }> => i.kind === 'channel')
                        .map((i) => i.idx);
                const autoNext = computeAutoSetConfig(win, st.config, connected);
                setConfig(wid, channelCount, autoNext);
              }}
            />
          </div>
        </div>
      );
    }
    case 'raw':
      return <RawDataView widgetId={tab.widgetId} />;
    case 'pie': {
      const widget = widgets.find(
        (w) => w.params.id === tab.widgetId && w.kind === 'PieChart'
      ) as Extract<WidgetConfig, { kind: 'PieChart' }> | undefined;
      if (!widget) return noWidget;
      return (
        <div className="flex h-full p-2">
          <PieChart widget={widget} onRemove={noopRemove} full />
        </div>
      );
    }
    case 'image': {
      const widget = widgets.find(
        (w) => w.params.id === tab.widgetId && w.kind === 'Image'
      ) as Extract<WidgetConfig, { kind: 'Image' }> | undefined;
      if (!widget) return noWidget;
      return (
        <div className="flex h-full p-2">
          <ImageViewer widget={widget} onRemove={noopRemove} full />
        </div>
      );
    }
    case 'model3d': {
      const widget = widgets.find(
        (w) => w.params.id === tab.widgetId && w.kind === 'Model3D'
      ) as Extract<WidgetConfig, { kind: 'Model3D' }> | undefined;
      if (!widget) return noWidget;
      return (
        <div className="flex h-full p-2">
          <Suspense fallback={<SuspenseFallback />}>
            <Model3DWidget widget={widget} onRemove={noopRemove} />
          </Suspense>
        </div>
      );
    }
    case 'spectrum': {
      const widget = widgets.find(
        (w) => w.params.id === tab.widgetId && w.kind === 'Spectrum'
      ) as Extract<WidgetConfig, { kind: 'Spectrum' }> | undefined;
      if (!widget) return noWidget;
      return (
        <div className="flex h-full p-2">
          <SpectrumChart widget={widget} onRemove={noopRemove} />
        </div>
      );
    }
    case 'command': {
      const widget = widgets.find(
        (w) => w.params.id === tab.widgetId && w.kind === 'Command'
      ) as Extract<WidgetConfig, { kind: 'Command' }> | undefined;
      if (!widget) return noWidget;
      return (
        <div className="flex h-full p-2">
          <CommandSender widget={widget} onRemove={noopRemove} />
        </div>
      );
    }
    case 'can':
      return (
        <div className="flex h-full w-full">
          <CanView />
        </div>
      );
    case 'logic':
      return (
        <div className="flex h-full w-full">
          <LogicView />
        </div>
      );
    case 'table-view': {
      const widget = widgets.find(
        (w) => w.params.id === tab.widgetId && w.kind === 'TableView'
      ) as Extract<WidgetConfig, { kind: 'TableView' }> | undefined;
      if (!widget) return noWidget;
      const cmdWidget = widgets.find(
        (w) => w.kind === 'Command' && w.params.loopbackEnabled
      ) as Extract<WidgetConfig, { kind: 'Command' }> | undefined;
      return (
        <div className="flex h-full w-full">
          <TableView widget={widget} onRemove={noopRemove} loopbackHistory={cmdWidget?.params.loopbackHistory} />
        </div>
      );
    }
    case 'frame-decoder': {
      const widget = widgets.find(
        (w) => w.params.id === tab.widgetId && w.kind === 'FrameDecoder'
      ) as Extract<WidgetConfig, { kind: 'FrameDecoder' }> | undefined;
      if (!widget) return noWidget;
      return (
        <div className="flex h-full w-full">
          <FrameDecoder widget={widget} onRemove={noopRemove} />
        </div>
      );
    }
    default:
      return null;
  }
}

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
    case 'table-view':
      return <BarChart3Icon size={size} />;
    default:
      return null;
  }
}
