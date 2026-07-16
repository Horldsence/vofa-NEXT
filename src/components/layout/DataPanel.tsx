import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { Activity, LineChart, SlidersHorizontal } from 'lucide-react';
import { useState } from 'react';
import { RawDataView } from '../displays/RawDataView';
import { WaveformChart } from '../displays/WaveformChart';
import { AxisSettings, DEFAULT_AXIS_CONFIG, type WaveformAxisConfig } from '../displays/AxisSettings';

/// 数据显示区 — 上方波形, 下方原始数据
/// 波形模式下右侧提供 X/Y 轴选择面板
export function DataPanel() {
  const lang = useAppStore((s) => s.lang);
  const widgets = useAppStore((s) => s.widgets);
  const protocolConfig = useAppStore((s) => s.protocolConfig);
  const [tab, setTab] = useState<'waveform' | 'raw'>('waveform');
  const [showAxisSettings, setShowAxisSettings] = useState(true);
  const [axisConfig, setAxisConfig] = useState<WaveformAxisConfig>(DEFAULT_AXIS_CONFIG);

  const waveformWidgets = widgets.filter((w) => w.kind === 'Waveform');
  const waveformWidget = waveformWidgets[0] ?? {
    kind: 'Waveform' as const,
    params: {
      id: 'default',
      channels: protocolConfig.kind === 'RawData' ? 4 : protocolConfig.channels,
      max_points: 10000,
      visible_channels: [true, true, true, true],
    },
  };

  return (
    <div className="panel">
      <div className="tabs">
        <div
          className={`tab ${tab === 'waveform' ? 'active' : ''}`}
          onClick={() => setTab('waveform')}
        >
          <LineChart size={12} />
          {t(lang, 'waveform')}
        </div>
        <div
          className={`tab ${tab === 'raw' ? 'active' : ''}`}
          onClick={() => setTab('raw')}
        >
          <Activity size={12} />
          {t(lang, 'rawData')}
        </div>
        <div style={{ flex: 1 }} />
        {tab === 'waveform' && (
          <button
            className={`btn-icon ${showAxisSettings ? 'active' : ''}`}
            style={showAxisSettings ? { color: 'var(--text-bright)' } : {}}
            title={t(lang, 'axisSettings')}
            onClick={() => setShowAxisSettings(!showAxisSettings)}
          >
            <SlidersHorizontal size={14} />
          </button>
        )}
      </div>
      <div className="panel-content">
        {tab === 'waveform' ? (
          <div className="waveform-layout">
            <div className="waveform-main">
              <WaveformChart widget={waveformWidget} axisConfig={axisConfig} />
            </div>
            <div
              className="waveform-sidebar"
              style={{
                width: showAxisSettings ? 220 : 0,
                opacity: showAxisSettings ? 1 : 0,
                overflow: showAxisSettings ? 'auto' : 'hidden',
                borderLeftWidth: showAxisSettings ? 1 : 0,
                padding: showAxisSettings ? undefined : 0,
              }}
            >
              <AxisSettings
                config={axisConfig}
                onChange={setAxisConfig}
                channelCount={waveformWidget.params.channels}
              />
            </div>
          </div>
        ) : (
          <RawDataView />
        )}
      </div>
    </div>
  );
}
