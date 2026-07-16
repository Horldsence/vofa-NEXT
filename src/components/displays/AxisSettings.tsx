import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { Eye, EyeOff, TestTube } from 'lucide-react';
import { waveformBuffer } from '../../lib/dataBuffer';

/// 轴配置类型
export interface AxisConfig {
  auto: boolean;
  min: number;
  max: number;
}

export interface WaveformAxisConfig {
  x: AxisConfig;
  y: AxisConfig;
  grid: boolean;
  visibleChannels: boolean[];
}

export const DEFAULT_AXIS_CONFIG: WaveformAxisConfig = {
  x: { auto: true, min: 0, max: 10 },
  y: { auto: true, min: -1, max: 1 },
  grid: true,
  visibleChannels: [true, true, true, true],
};

interface AxisSettingsProps {
  config: WaveformAxisConfig;
  onChange: (config: WaveformAxisConfig) => void;
  channelCount: number;
}

/// 坐标轴设置面板 — X/Y 轴范围、网格、通道可见性
export function AxisSettings({ config, onChange, channelCount }: AxisSettingsProps) {
  const lang = useAppStore((s) => s.lang);

  const updateAxis = (axis: 'x' | 'y', patch: Partial<AxisConfig>) => {
    onChange({ ...config, [axis]: { ...config[axis], ...patch } });
  };

  const toggleChannel = (idx: number) => {
    const next = [...config.visibleChannels];
    next[idx] = !next[idx];
    onChange({ ...config, visibleChannels: next });
  };

  return (
    <div className="axis-settings">
      <div className="axis-settings-header">
        {t(lang, 'axisSettings')}
      </div>
      <div className="axis-settings-body">
        {/* X 轴 */}
        <div className="form-group">
          <label className="form-label">{t(lang, 'xAxis')}</label>
          <div className="axis-mode-toggle">
            <label className="radio-item">
              <input
                type="radio"
                name="x-mode"
                checked={config.x.auto}
                onChange={() => updateAxis('x', { auto: true })}
              />
              <span>{t(lang, 'autoRange')}</span>
            </label>
            <label className="radio-item">
              <input
                type="radio"
                name="x-mode"
                checked={!config.x.auto}
                onChange={() => updateAxis('x', { auto: false })}
              />
              <span>{t(lang, 'manualRange')}</span>
            </label>
          </div>
          {!config.x.auto && (
            <div className="form-row">
              <input
                type="number"
                value={config.x.min}
                onChange={(e) =>
                  updateAxis('x', { min: parseFloat(e.target.value) || 0 })
                }
                placeholder="min"
              />
              <input
                type="number"
                value={config.x.max}
                onChange={(e) =>
                  updateAxis('x', { max: parseFloat(e.target.value) || 0 })
                }
                placeholder="max"
              />
            </div>
          )}
        </div>

        {/* Y 轴 */}
        <div className="form-group">
          <label className="form-label">{t(lang, 'yAxis')}</label>
          <div className="axis-mode-toggle">
            <label className="radio-item">
              <input
                type="radio"
                name="y-mode"
                checked={config.y.auto}
                onChange={() => updateAxis('y', { auto: true })}
              />
              <span>{t(lang, 'autoRange')}</span>
            </label>
            <label className="radio-item">
              <input
                type="radio"
                name="y-mode"
                checked={!config.y.auto}
                onChange={() => updateAxis('y', { auto: false })}
              />
              <span>{t(lang, 'manualRange')}</span>
            </label>
          </div>
          {!config.y.auto && (
            <div className="form-row">
              <input
                type="number"
                value={config.y.min}
                onChange={(e) =>
                  updateAxis('y', { min: parseFloat(e.target.value) || 0 })
                }
                placeholder="min"
              />
              <input
                type="number"
                value={config.y.max}
                onChange={(e) =>
                  updateAxis('y', { max: parseFloat(e.target.value) || 0 })
                }
                placeholder="max"
              />
            </div>
          )}
        </div>

        {/* 网格 */}
        <div className="form-group">
          <label className="radio-item">
            <input
              type="checkbox"
              checked={config.grid}
              onChange={(e) => onChange({ ...config, grid: e.target.checked })}
            />
            <span>{t(lang, 'gridVisible')}</span>
          </label>
        </div>

        {/* 通道可见性 */}
        <div className="form-group">
          <label className="form-label">{t(lang, 'channelVisible')}</label>
          <div className="channel-toggle-list">
            {Array.from({ length: channelCount }).map((_, i) => {
              const visible = config.visibleChannels[i] ?? true;
              return (
                <button
                  key={i}
                  className={`channel-toggle ${visible ? 'on' : 'off'}`}
                  onClick={() => toggleChannel(i)}
                  title={`CH${i}`}
                >
                  {visible ? <Eye size={12} /> : <EyeOff size={12} />}
                  <span>CH{i}</span>
                </button>
              );
            })}
          </div>
        </div>

        {/* 调试: 注入测试波形 */}
        <div className="form-group" style={{ marginTop: 12, paddingTop: 12, borderTop: '1px solid var(--border)' }}>
          <button
            className="btn w-full"
            onClick={() => waveformBuffer.injectTestData(500)}
          >
            <TestTube size={12} />
            {t(lang, 'injectTestData')}
          </button>
        </div>
      </div>
    </div>
  );
}
