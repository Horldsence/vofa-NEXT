import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { useContextMenu, showContextMenu } from '../../lib/useContextMenu';
import { LineChart, Activity, PieChart as PieIcon, Image as ImageIcon, Box, BarChart3, Send, X, Cpu, CircuitBoard, Trash2, ScanText } from 'lucide-react';
import { WaveformChart } from '../displays/WaveformChart';
import { RawDataView } from '../displays/RawDataView';
import { PieChart } from '../displays/PieChart';
import { ImageViewer } from '../displays/ImageViewer';
import { Model3DWidget } from '../displays/Model3DWidget';
import { SpectrumChart } from '../displays/SpectrumChart';
import { CommandSender } from '../displays/CommandSender';
import { CanView } from '../displays/CanView';
import { LogicView } from '../displays/LogicView';
import { FrameDecoder } from '../displays/FrameDecoder';
import { AxisSettings } from '../displays/AxisSettings';
import { useState, useEffect, useCallback, useMemo } from 'react';
import type { WidgetConfig, ScopeAxisConfig, ScopeMeasurements, ProtocolConfig } from '../../types';
import { createDefaultScopeConfig } from '../../types';
import { waveformWindow } from '../../lib/dataBuffer';
import { computeMeasurements, computeAutoSetConfig } from '../../lib/scopeUtils';
/// 每个 waveform widget 拥有独立的 axisConfig + measurements
/// 通过 widgetId 索引, 切换 Tab 时使用对应配置, 互不干扰
interface PerWidgetState {
  config: ScopeAxisConfig;
  measurements: ScopeMeasurements | null;
  lastMeasureVersion: number;
}

/// 创建 per-widget state (懒初始化)
function createPerWidgetState(channelCount: number): PerWidgetState {
  return {
    config: createDefaultScopeConfig(channelCount),
    measurements: null,
    lastMeasureVersion: -1,
  };
}

/// 数据显示区 — MATLAB 风格多 Tab Figure
/// 固定 Tab: 波形图 + 原始数据
/// 动态 Tab: 显示控件节点对应的 Figure
/// 每个 waveform Tab 拥有独立的 axisConfig (修复: 之前所有示波器共用一个 axisConfig)
export function DataPanel() {
  const lang = useAppStore((s) => s.lang);
  const dataTabs = useAppStore((s) => s.dataTabs);
  const activeDataTabId = useAppStore((s) => s.activeDataTabId);
  const setActiveDataTab = useAppStore((s) => s.setActiveDataTab);
  const removeDataTab = useAppStore((s) => s.removeDataTab);
  const widgets = useAppStore((s) => s.widgets);
  const protocolConfig = useAppStore((s) => s.protocolConfig);
  const detectedChannels = useAppStore((s) => s.detectedChannels);
  // 不订阅 rawDataVersion: DataPanel 本身不需要 raw data 刷新, channel_count 仅在协议/检测变化时改变
  const winChannelCount = waveformWindow.get().channel_count;

  // 每个 waveform widget 独立配置, key = widgetId (固定 Tab 用 'default-waveform')
  const [perWidgetStates, setPerWidgetStates] = useState<Record<string, PerWidgetState>>({
    'default-waveform': createPerWidgetState(4),
  });

  // 计算默认波形的通道数: 自动模式优先用检测到的通道数, 其次用窗口缓存, 最后兜底 4
  // 用 useMemo 稳定, 避免每次渲染创建新的 defaultWaveformWidget 导致 WaveformChart 重新计算
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

  // 当前活动 Tab 的 widgetId
  const activeTab = dataTabs.find((t) => t.id === activeDataTabId);
  const isWaveformTab = activeTab?.type === 'waveform' || activeTab?.type === 'waveform-extra';
  const activeWidgetId =
    isWaveformTab
      ? (activeTab?.widgetId ?? 'default-waveform')
      : 'default-waveform';
  const activeWidget =
    isWaveformTab && activeTab?.widgetId
      ? (widgets.find(
          (w) => w.params.id === activeTab.widgetId && w.kind === 'Waveform'
        ) as Extract<WidgetConfig, { kind: 'Waveform' }> | undefined)
      : undefined;
  const activeWaveWidget = activeWidget ?? defaultWaveformWidget;
  const activeChannelCount = activeWaveWidget.params.channels;

  // 确保 perWidgetStates 中存在当前 widgetId 的配置 (懒初始化)
  useEffect(() => {
    setPerWidgetStates((prev) => {
      if (prev[activeWidgetId]) {
        // 已存在: 检查通道数是否需要扩展
        const existing = prev[activeWidgetId];
        if (existing.config.channels.length >= activeChannelCount) return prev;
        const nextCh = existing.config.channels.slice();
        while (nextCh.length < activeChannelCount) {
          nextCh.push({ vPerDiv: 1, position: 0, show: true, coupling: 'DC' });
        }
        return { ...prev, [activeWidgetId]: { ...existing, config: { ...existing.config, channels: nextCh } } };
      }
      return { ...prev, [activeWidgetId]: createPerWidgetState(activeChannelCount) };
    });
  }, [activeWidgetId, activeChannelCount]);

  // 移除 widget 时清理其配置 (随 widgets 变化)
  useEffect(() => {
    setPerWidgetStates((prev) => {
      const next = { ...prev };
      // 保留 default-waveform, 删除已不存在的 widget 配置
      for (const wid of Object.keys(next)) {
        if (wid === 'default-waveform') continue;
        const exists = widgets.some((w) => w.params.id === wid);
        if (!exists) delete next[wid];
      }
      return next;
    });
  }, [widgets]);

  // 当前激活的 state (没有则用 default 的兜底)
  const activeState: PerWidgetState = perWidgetStates[activeWidgetId] ?? perWidgetStates['default-waveform'] ?? createPerWidgetState(activeChannelCount);
  const axisConfig = activeState.config;

  // 更新当前 widget 的 config
  const setAxisConfig = useCallback(
    (next: ScopeAxisConfig) => {
      setPerWidgetStates((prev) => {
        const cur = prev[activeWidgetId] ?? createPerWidgetState(activeChannelCount);
        return { ...prev, [activeWidgetId]: { ...cur, config: next } };
      });
    },
    [activeWidgetId, activeChannelCount]
  );
  void setAxisConfig;

  // 计算测量值 (基于第一可见通道, 节流到波形版本变化) — 仅在当前 active widget 触发
  useEffect(() => {
    if (!axisConfig.running) return;
    const tick = () => {
      const version = waveformWindow.version;
      const cur = perWidgetStates[activeWidgetId];
      if (!cur) return;
      if (version !== cur.lastMeasureVersion) {
        const win = waveformWindow.get();
        let m: ScopeMeasurements | null = null;
        if (win.timestamps.length >= 2) {
          const chIdx = cur.config.channels.findIndex((c) => c.show);
          const targetIdx = chIdx >= 0 ? chIdx : 0;
          const ch = win.channels[targetIdx];
          if (ch && ch.length > 0) {
            m = computeMeasurements(ch, win.timestamps);
          }
        }
        setPerWidgetStates((prev) => ({
          ...prev,
          [activeWidgetId]: {
            ...(prev[activeWidgetId] ?? createPerWidgetState(activeChannelCount)),
            lastMeasureVersion: version,
            measurements: m,
          },
        }));
      }
    };
    const raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [axisConfig.running, axisConfig.channels, activeWidgetId, activeChannelCount, perWidgetStates]);

  const renderTabContent = () => {
    const tab = dataTabs.find((t) => t.id === activeDataTabId);
    if (!tab) return null;

    switch (tab.type) {
      case 'waveform':
      case 'waveform-extra': {
        // 固定波形 Tab 使用默认控件，动态 Tab 使用关联控件
        const widget =
          tab.widgetId
            ? (widgets.find(
                (w) => w.params.id === tab.widgetId && w.kind === 'Waveform'
              ) as Extract<WidgetConfig, { kind: 'Waveform' }> | undefined)
            : undefined;
        const waveWidget = widget ?? defaultWaveformWidget;
        // 每个 widget 独立 axisConfig: 此处取该 widget 的 state
        const wid = waveWidget.params.id;
        const st = perWidgetStates[wid] ?? createPerWidgetState(waveWidget.params.channels);
        return (
          <div className="flex h-full w-full">
            <div className="flex-1 min-w-0 relative">
              <WaveformChart
                widget={waveWidget}
                axisConfig={st.config}
                onConfigChange={(next) => {
                  setPerWidgetStates((prev) => {
                    const cur = prev[wid] ?? createPerWidgetState(waveWidget.params.channels);
                    return { ...prev, [wid]: { ...cur, config: next } };
                  });
                }}
              />
            </div>
            <div className="w-[240px] flex-shrink-0 border-l border-border bg-bg-sidebar overflow-y-auto">
              <AxisSettings
                config={st.config}
                onChange={(next) => {
                  setPerWidgetStates((prev) => {
                    const cur = prev[wid] ?? createPerWidgetState(waveWidget.params.channels);
                    return { ...prev, [wid]: { ...cur, config: next } };
                  });
                }}
                channelCount={waveWidget.params.channels}
                measurements={st.measurements}
                onAutoSet={() => {
                  const win = waveformWindow.get();
                  const connected =
                    wid === 'default-waveform'
                      ? Array.from({ length: win.channel_count || waveWidget.params.channels }, (_, i) => i)
                      : [];
                  const autoNext = computeAutoSetConfig(win, st.config, connected);
                  setPerWidgetStates((prev) => {
                    const cur = prev[wid] ?? createPerWidgetState(waveWidget.params.channels);
                    return { ...prev, [wid]: { ...cur, config: autoNext } };
                  });
                }}
              />
            </div>
          </div>
        );
      }
      case 'raw':
        return <RawDataView />;
      case 'pie': {
        const widget = widgets.find(
          (w) => w.params.id === tab.widgetId && w.kind === 'PieChart'
        ) as Extract<WidgetConfig, { kind: 'PieChart' }> | undefined;
        if (!widget) return <div className="flex items-center justify-center h-full text-text-secondary text-sm">{t(lang, 'noWidgets')}</div>;
        return (
          <div className="flex h-full p-2">
            <PieChart widget={widget} onRemove={() => {}} full />
          </div>
        );
      }
      case 'image': {
        const widget = widgets.find(
          (w) => w.params.id === tab.widgetId && w.kind === 'Image'
        ) as Extract<WidgetConfig, { kind: 'Image' }> | undefined;
        if (!widget) return <div className="flex items-center justify-center h-full text-text-secondary text-sm">{t(lang, 'noWidgets')}</div>;
        return (
          <div className="flex h-full p-2">
            <ImageViewer widget={widget} onRemove={() => {}} full />
          </div>
        );
      }
      case 'model3d': {
        const widget = widgets.find(
          (w) => w.params.id === tab.widgetId && w.kind === 'Model3D'
        ) as Extract<WidgetConfig, { kind: 'Model3D' }> | undefined;
        if (!widget) return <div className="flex items-center justify-center h-full text-text-secondary text-sm">{t(lang, 'noWidgets')}</div>;
        return (
          <div className="flex h-full p-2">
            <Model3DWidget widget={widget} onRemove={() => {}} />
          </div>
        );
      }
      case 'spectrum': {
        const widget = widgets.find(
          (w) => w.params.id === tab.widgetId && w.kind === 'Spectrum'
        ) as Extract<WidgetConfig, { kind: 'Spectrum' }> | undefined;
        if (!widget) return <div className="flex items-center justify-center h-full text-text-secondary text-sm">{t(lang, 'noWidgets')}</div>;
        return (
          <div className="flex h-full p-2">
            <SpectrumChart widget={widget} onRemove={() => {}} />
          </div>
        );
      }
      case 'command': {
        const widget = widgets.find(
          (w) => w.params.id === tab.widgetId && w.kind === 'Command'
        ) as Extract<WidgetConfig, { kind: 'Command' }> | undefined;
        if (!widget) return <div className="flex items-center justify-center h-full text-text-secondary text-sm">{t(lang, 'noWidgets')}</div>;
        return (
          <div className="flex h-full p-2">
            <CommandSender widget={widget} onRemove={() => {}} />
          </div>
        );
      }
      case 'can': {
        return (
          <div className="flex h-full w-full">
            <CanView />
          </div>
        );
      }
      case 'logic': {
        return (
          <div className="flex h-full w-full">
            <LogicView />
          </div>
        );
      }
      case 'frame-decoder': {
        const widget = widgets.find(
          (w) => w.params.id === tab.widgetId && w.kind === 'FrameDecoder'
        ) as Extract<WidgetConfig, { kind: 'FrameDecoder' }> | undefined;
        if (!widget) return <div className="flex items-center justify-center h-full text-text-secondary text-sm">{t(lang, 'noWidgets')}</div>;
        return (
          <div className="flex h-full w-full">
            <FrameDecoder widget={widget} onRemove={() => {}} />
          </div>
        );
      }
      default:
        return null;
    }
  };

  const getTabIcon = (type: string) => {
    switch (type) {
      case 'waveform':
      case 'waveform-extra':
        return <LineChart size={12} />;
      case 'raw':
        return <Activity size={12} />;
      case 'pie':
        return <PieIcon size={12} />;
      case 'image':
        return <ImageIcon size={12} />;
      case 'model3d':
        return <Box size={12} />;
      case 'spectrum':
        return <BarChart3 size={12} />;
      case 'command':
        return <Send size={12} />;
      case 'can':
        return <Cpu size={12} />;
      case 'logic':
        return <CircuitBoard size={12} />;
      case 'frame-decoder':
        return <ScanText size={12} />;
      default:
        return null;
    }
  };

  const hasCanTab = dataTabs.some((t) => t.type === 'can');
  const hasLogicTab = dataTabs.some((t) => t.type === 'logic');

  const tabBarContextMenu = useContextMenu([
    {
      id: 'add-can-tab',
      label: t(lang, 'addCanTab'),
      icon: <Cpu size={14} />,
      disabled: hasCanTab,
      onClick: () => useAppStore.getState().addCanTab(),
    },
    {
      id: 'add-logic-tab',
      label: t(lang, 'addLogicTab'),
      icon: <CircuitBoard size={14} />,
      disabled: hasLogicTab,
      onClick: () => useAppStore.getState().addLogicTab(),
    },
  ]);

  const makeTabContextMenu = useCallback(
    (tabId: string) => {
      const tab = dataTabs.find((t) => t.id === tabId);
      if (!tab) return [];
      const otherClosableTabs = dataTabs.filter((t) => t.id !== tabId && t.closable);
      return [
        {
          id: 'close',
          label: t(lang, 'contextMenuCloseTab'),
          icon: <Trash2 size={14} />,
          disabled: !tab.closable,
          onClick: () => removeDataTab(tabId),
        },
        {
          id: 'close-others',
          label: t(lang, 'contextMenuCloseOtherTabs'),
          icon: <X size={14} />,
          disabled: otherClosableTabs.length === 0,
          onClick: () => {
            otherClosableTabs.forEach((t) => removeDataTab(t.id));
          },
        },
      ];
    },
    [dataTabs, lang, removeDataTab]
  );

  return (
    <div className="flex flex-col bg-bg-editor overflow-hidden h-full w-full">
      <div className="flex bg-bg-panel-header border-b border-border flex-shrink-0" data-tour="data-tabs" onContextMenu={tabBarContextMenu}>
        {dataTabs.map((tab) => (
          <div
            key={tab.id}
            className={`px-3 h-7 text-xs cursor-pointer border-r border-border flex items-center gap-1 hover:bg-bg-hover transition-colors ${tab.id === activeDataTabId ? 'text-text-bright bg-bg-editor border-t-2 border-t-accent' : 'text-text-secondary'}`}
            onClick={() => setActiveDataTab(tab.id)}
            onContextMenu={(e) => {
              e.preventDefault();
              e.stopPropagation();
              const items = makeTabContextMenu(tab.id);
              if (items.length > 0) {
                showContextMenu(e.clientX, e.clientY, items);
              }
            }}
          >
            {getTabIcon(tab.type)}
            <span>{tab.name}</span>
            {tab.closable && (
              <button
                className="w-4 h-4 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer ml-0.5 p-0"
                onClick={(e) => {
                  e.stopPropagation();
                  removeDataTab(tab.id);
                }}
              >
                <X size={10} />
              </button>
            )}
          </div>
        ))}
        <button
          className="px-2 h-7 text-xs cursor-pointer flex items-center text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors"
          onClick={() => useAppStore.getState().addCanTab()}
          title={t(useAppStore.getState().lang, 'addCanTab')}
        >
          <Cpu size={12} />
        </button>
        <button
          className="px-2 h-7 text-xs cursor-pointer flex items-center text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors"
          onClick={() => useAppStore.getState().addLogicTab()}
          title={t(useAppStore.getState().lang, 'addLogicTab')}
        >
          <CircuitBoard size={12} />
        </button>
      </div>
      <div className="flex-1 overflow-hidden relative min-h-0">
        {renderTabContent()}
      </div>
    </div>
  );
}
