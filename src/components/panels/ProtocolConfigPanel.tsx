import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { ArrowRight } from 'lucide-react';
import type { ProtocolConfig } from '../../types';

/// 协议引擎配置面板 — JustFloat / FireWater / RawData
/// 支持通道数: 自动检测 (null) 或 手动指定 (number)
export function ProtocolConfigPanel() {
  const lang = useAppStore((s) => s.lang);
  const protocolConfig = useAppStore((s) => s.protocolConfig);
  const setProtocolConfig = useAppStore((s) => s.setProtocolConfig);
  const setSidebarView = useAppStore((s) => s.setSidebarView);
  const detectedChannels = useAppStore((s) => s.detectedChannels);

  const isRaw = protocolConfig.kind === 'RawData';
  const isAuto = !isRaw && protocolConfig.channels == null;

  const updateKind = (kind: ProtocolConfig['kind']) => {
    if (kind === 'RawData') {
      setProtocolConfig({ kind: 'RawData' });
    } else {
      // 切换协议引擎时保留原通道配置 (auto / manual)
      const prevChannels = protocolConfig.kind === 'RawData' ? null : protocolConfig.channels;
      setProtocolConfig({ kind, channels: prevChannels });
    }
  };

  /// 切换自动 / 手动模式
  const setAutoMode = (auto: boolean) => {
    if (isRaw) return;
    setProtocolConfig({
      kind: protocolConfig.kind,
      channels: auto ? null : 4,
    });
  };

  /// 手动模式下更新通道数
  const updateManualChannels = (channels: number) => {
    if (isRaw) return;
    const clamped = Math.max(1, Math.min(32, Math.floor(channels) || 1));
    setProtocolConfig({ kind: protocolConfig.kind, channels: clamped });
  };

  const kinds: { value: ProtocolConfig['kind']; label: string }[] = [
    { value: 'JustFloat', label: t(lang, 'justfloat') },
    { value: 'FireWater', label: t(lang, 'firewater') },
    { value: 'RawData', label: t(lang, 'rawdata') },
  ];

  return (
    <div>
      <div className="mb-2.5">
        <label className="block text-xs text-text-secondary mb-1">{t(lang, 'protocolEngine')}</label>
        <select
          value={protocolConfig.kind}
          onChange={(e) => updateKind(e.target.value as ProtocolConfig['kind'])}
          className="w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded text-sm focus:outline-none focus:border-accent transition-colors"
        >
          {kinds.map((k) => (
            <option key={k.value} value={k.value}>
              {k.label}
            </option>
          ))}
        </select>
      </div>

      {!isRaw && (
        <>
          <div className="mb-2.5">
            <label className="block text-xs text-text-secondary mb-1">{t(lang, 'channels')}</label>
            <div className="flex flex-col gap-1">
              <label className="flex items-center gap-1.5 cursor-pointer text-sm">
                <input
                  type="radio"
                  name="channel-mode"
                  checked={isAuto}
                  onChange={() => setAutoMode(true)}
                  className="accent-accent"
                />
                <span>{t(lang, 'channelsAuto')}</span>
              </label>
              <label className="flex items-center gap-1.5 cursor-pointer text-sm">
                <input
                  type="radio"
                  name="channel-mode"
                  checked={!isAuto}
                  onChange={() => setAutoMode(false)}
                  className="accent-accent"
                />
                <span>{t(lang, 'channelsManual')}</span>
              </label>
            </div>
          </div>

          {!isAuto && (
            <div className="mb-2.5">
              <input
                type="number"
                min={1}
                max={32}
                value={protocolConfig.channels ?? 4}
                onChange={(e) => updateManualChannels(parseInt(e.target.value) || 1)}
                className="w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded text-sm focus:outline-none focus:border-accent transition-colors"
              />
            </div>
          )}

          {isAuto && (
            <div className="mt-1 px-2 py-1.5 bg-bg-input rounded text-xs text-text-secondary flex justify-between items-center">
              <span>{t(lang, 'detectedChannels')}:</span>
              <span className="text-blue font-mono">
                {detectedChannels != null ? detectedChannels : '--'}
              </span>
            </div>
          )}
        </>
      )}

      <div className="mt-3 p-2 bg-bg-input rounded text-xs text-text-secondary leading-relaxed">
        {protocolConfig.kind === 'JustFloat' && (
          <>
            <strong className="text-text-primary">JustFloat</strong>
            <br />
            {lang === 'zh'
              ? '4 字节小端浮点数 + 帧尾 [0x00,0x00,0x80,0x7f]。适合高速波形传输。'
              : '4-byte LE floats + tail [0x00,0x00,0x80,0x7f]. High-throughput waveform.'}
          </>
        )}
        {protocolConfig.kind === 'FireWater' && (
          <>
            <strong className="text-text-primary">FireWater</strong>
            <br />
            {lang === 'zh'
              ? 'CSV 格式, 通道间逗号分隔, 以 \\n 结尾。可读性强。'
              : 'CSV format, channels separated by commas, ends with \\n. Human-readable.'}
          </>
        )}
        {protocolConfig.kind === 'RawData' && (
          <>
            <strong className="text-text-primary">RawData</strong>
            <br />
            {lang === 'zh'
              ? '原始字节流, 不解析。仅显示原始数据。'
              : 'Raw byte stream, no parsing. Raw data only.'}
          </>
        )}
      </div>

      {/* 跳转到串口配置 */}
      <div className="mb-2.5 mt-4">
        <button className="w-full px-3 py-1.5 bg-bg-button text-text-bright border-none rounded cursor-pointer text-sm text-center transition-colors hover:bg-bg-button-hover inline-flex items-center justify-center gap-1.5" onClick={() => setSidebarView('port')}>
          {t(lang, 'nextPort')}
          <ArrowRight size={14} />
        </button>
      </div>
    </div>
  );
}
