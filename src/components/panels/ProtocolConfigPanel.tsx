import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { ArrowRight } from 'lucide-react';
import type { ProtocolConfig } from '../../types';

/// 协议引擎配置面板 — JustFloat / FireWater / RawData
export function ProtocolConfigPanel() {
  const lang = useAppStore((s) => s.lang);
  const protocolConfig = useAppStore((s) => s.protocolConfig);
  const setProtocolConfig = useAppStore((s) => s.setProtocolConfig);
  const setSidebarView = useAppStore((s) => s.setSidebarView);

  const updateKind = (kind: ProtocolConfig['kind']) => {
    if (kind === 'RawData') {
      setProtocolConfig({ kind: 'RawData' });
    } else {
      const channels =
        protocolConfig.kind === 'RawData' ? 4 : protocolConfig.channels;
      setProtocolConfig({ kind, channels });
    }
  };

  const updateChannels = (channels: number) => {
    if (protocolConfig.kind === 'RawData') return;
    setProtocolConfig({ ...protocolConfig, channels });
  };

  const kinds: { value: ProtocolConfig['kind']; label: string }[] = [
    { value: 'JustFloat', label: t(lang, 'justfloat') },
    { value: 'FireWater', label: t(lang, 'firewater') },
    { value: 'RawData', label: t(lang, 'rawdata') },
  ];

  const currentChannels =
    protocolConfig.kind === 'RawData' ? 0 : protocolConfig.channels;

  return (
    <div>
      <div className="form-group">
        <label className="form-label">{t(lang, 'protocolEngine')}</label>
        <select
          value={protocolConfig.kind}
          onChange={(e) => updateKind(e.target.value as ProtocolConfig['kind'])}
        >
          {kinds.map((k) => (
            <option key={k.value} value={k.value}>
              {k.label}
            </option>
          ))}
        </select>
      </div>

      {protocolConfig.kind !== 'RawData' && (
        <div className="form-group">
          <label className="form-label">{t(lang, 'channels')}</label>
          <input
            type="number"
            min={1}
            max={32}
            value={currentChannels}
            onChange={(e) => updateChannels(parseInt(e.target.value) || 1)}
          />
        </div>
      )}

      <div
        style={{
          marginTop: 12,
          padding: 8,
          background: 'var(--bg-input)',
          borderRadius: 4,
          fontSize: 11,
          color: 'var(--text-secondary)',
          lineHeight: 1.6,
        }}
      >
        {protocolConfig.kind === 'JustFloat' && (
          <>
            <strong style={{ color: 'var(--text-primary)' }}>JustFloat</strong>
            <br />
            {lang === 'zh'
              ? '4 字节小端浮点数 + 帧尾 [0x00,0x00,0x80,0x7f]。适合高速波形传输。'
              : '4-byte LE floats + tail [0x00,0x00,0x80,0x7f]. High-throughput waveform.'}
          </>
        )}
        {protocolConfig.kind === 'FireWater' && (
          <>
            <strong style={{ color: 'var(--text-primary)' }}>FireWater</strong>
            <br />
            {lang === 'zh'
              ? 'CSV 格式, 通道间逗号分隔, 以 \\n 结尾。可读性强。'
              : 'CSV format, channels separated by commas, ends with \\n. Human-readable.'}
          </>
        )}
        {protocolConfig.kind === 'RawData' && (
          <>
            <strong style={{ color: 'var(--text-primary)' }}>RawData</strong>
            <br />
            {lang === 'zh'
              ? '原始字节流, 不解析。仅显示原始数据。'
              : 'Raw byte stream, no parsing. Raw data only.'}
          </>
        )}
      </div>

      {/* 跳转到串口配置 */}
      <div className="form-group" style={{ marginTop: 16 }}>
        <button
          className="btn w-full"
          onClick={() => setSidebarView('port')}
        >
          {t(lang, 'nextPort')}
          <ArrowRight size={14} />
        </button>
      </div>
    </div>
  );
}
