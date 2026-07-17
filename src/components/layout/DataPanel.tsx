import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { LineChart, Activity, PieChart as PieIcon, Image as ImageIcon, X } from 'lucide-react';
import { WaveformChart } from '../displays/WaveformChart';
import { RawDataView } from '../displays/RawDataView';
import { PieChart } from '../displays/PieChart';
import { ImageViewer } from '../displays/ImageViewer';
import { AxisSettings } from '../displays/AxisSettings';
import { useState, useEffect, useCallback, useRef } from 'react';
import type { WidgetConfig, ScopeAxisConfig, ScopeMeasurements } from '../../types';
import { createDefaultScopeConfig } from '../../types';
import { waveformWindow } from '../../lib/dataBuffer';
import { computeMeasurements, computeAutoSetConfig } from '../../lib/scopeUtils';

/// 数据显示区 — MATLAB 风格多 Tab Figure
/// 固定 Tab: 波形图 + 原始数据
/// 动态 Tab: 显示控件节点对应的 Figure
export function DataPanel() {
  const lang = useAppStore((s) => s.lang);
  const dataTabs = useAppStore((s) => s.dataTabs);
  const activeDataTabId = useAppStore((s) => s.activeDataTabId);
  const setActiveDataTab = useAppStore((s) => s.setActiveDataTab);
  const removeDataTab = useAppStore((s) => s.removeDataTab);
  const widgets = useAppStore((s) => s.widgets);
  const protocolConfig = useAppStore((s) => s.protocolConfig);
  const detectedChannels = useAppStore((s) => s.detectedChannels);
  // 订阅 waveformWindow 版本以触发 rerender (获取最新 channel_count)
  const waveformVersion = useAppStore((s) => s.rawDataVersion);
  void waveformVersion;
  const winChannelCount = waveformWindow.get().channel_count;

  const [axisConfig, setAxisConfig] = useState<ScopeAxisConfig>(() =>
    createDefaultScopeConfig(4)
  );
  const [measurements, setMeasurements] = useState<ScopeMeasurements | null>(null);
  /// 上次计算测量值时的 waveformWindow 版本, 避免每帧重算
  const lastMeasureVersionRef = useRef(-1);

  // 计算默认波形的通道数: 自动模式优先用检测到的通道数, 其次用窗口缓存, 最后兜底 4
  const defaultChannelCount =
    protocolConfig.kind === 'RawData'
      ? 4
      : (protocolConfig.channels ?? detectedChannels ?? (winChannelCount || 4));

  // 通道数变化时扩展 channels 数组 (保留已有配置)
  useEffect(() => {
    setAxisConfig((prev) => {
      if (prev.channels.length >= defaultChannelCount) return prev;
      const next = prev.channels.slice();
      while (next.length < defaultChannelCount) {
        next.push({ vPerDiv: 1, position: 0, show: true, coupling: 'DC' });
      }
      return { ...prev, channels: next };
    });
  }, [defaultChannelCount]);

  // 默认波形控件（固定 Tab 使用）
  const defaultWaveformWidget: Extract<WidgetConfig, { kind: 'Waveform' }> = {
    kind: 'Waveform',
    params: {
      id: 'default-waveform',
      channels: defaultChannelCount,
      max_points: 10000,
      visible_channels: Array.from({ length: defaultChannelCount }, () => true),
    },
  };

  // 计算测量值 (基于第一可见通道, 节流到波形版本变化)
  useEffect(() => {
    if (!axisConfig.running) return;
    const tick = () => {
      const version = waveformWindow.version;
      if (version !== lastMeasureVersionRef.current) {
        lastMeasureVersionRef.current = version;
        const win = waveformWindow.get();
        if (win.timestamps.length < 2) {
          setMeasurements(null);
          return;
        }
        // 找到第一个 show=true 的通道
        const chIdx = axisConfig.channels.findIndex((c) => c.show);
        const targetIdx = chIdx >= 0 ? chIdx : 0;
        const ch = win.channels[targetIdx];
        if (!ch || ch.length === 0) {
          setMeasurements(null);
          return;
        }
        const m = computeMeasurements(ch, win.timestamps);
        setMeasurements(m);
      }
    };
    const raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [axisConfig.running, axisConfig.channels]);

  // Auto Set: 基于当前 waveformWindow 数据自动适配时基与每通道 V/div
  const handleAutoSet = useCallback(() => {
    const win = waveformWindow.get();
    const connected =
      defaultWaveformWidget.params.id === 'default-waveform'
        ? Array.from({ length: win.channel_count || defaultChannelCount }, (_, i) => i)
        : [];
    const next = computeAutoSetConfig(win, axisConfig, connected);
    setAxisConfig(next);
  }, [axisConfig, defaultChannelCount, defaultWaveformWidget.params.id]);

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
        return (
          <div className="waveform-layout">
            <div className="waveform-main">
              <WaveformChart
                widget={waveWidget}
                axisConfig={axisConfig}
                onConfigChange={setAxisConfig}
              />
            </div>
            <div className="waveform-sidebar" style={{ width: 240 }}>
              <AxisSettings
                config={axisConfig}
                onChange={setAxisConfig}
                channelCount={waveWidget.params.channels}
                measurements={measurements}
                onAutoSet={handleAutoSet}
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
        if (!widget) return <div className="empty-state">{t(lang, 'noWidgets')}</div>;
        return <PieChart widget={widget} onRemove={() => {}} />;
      }
      case 'image': {
        const widget = widgets.find(
          (w) => w.params.id === tab.widgetId && w.kind === 'Image'
        ) as Extract<WidgetConfig, { kind: 'Image' }> | undefined;
        if (!widget) return <div className="empty-state">{t(lang, 'noWidgets')}</div>;
        return <ImageViewer widget={widget} onRemove={() => {}} />;
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
      default:
        return null;
    }
  };

  return (
    <div className="panel">
      <div className="tabs">
        {dataTabs.map((tab) => (
          <div
            key={tab.id}
            className={`tab ${tab.id === activeDataTabId ? 'active' : ''}`}
            onClick={() => setActiveDataTab(tab.id)}
            style={{ cursor: 'pointer' }}
          >
            {getTabIcon(tab.type)}
            <span>{tab.name}</span>
            {tab.closable && (
              <button
                className="btn-icon"
                style={{ marginLeft: 2, padding: 0, width: 16, height: 16 }}
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
      </div>
      <div className="panel-content">
        {renderTabContent()}
      </div>
    </div>
  );
}
