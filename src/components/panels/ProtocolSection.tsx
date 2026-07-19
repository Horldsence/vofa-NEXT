import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { Info, Play, Square } from 'lucide-react';
import type { ProtocolConfig, LogicDecoderConfig } from '../../types';

/// 协议引擎配置面板
///
/// 包含: 协议类型选择 / 通道配置 (JustFloat/FireWater) / 解码器参数 (LogicDecode)
export function ProtocolSection() {
  const lang = useAppStore((s) => s.lang);
  const protocolConfig = useAppStore((s) => s.protocolConfig);
  const setProtocolConfig = useAppStore((s) => s.setProtocolConfig);
  const detectedChannels = useAppStore((s) => s.detectedChannels);
  const transportConfig = useAppStore((s) => s.transportConfig);
  const connectionState = useAppStore((s) => s.connectionState);
  const testDataRunning = useAppStore((s) => s.testDataRunning);
  const startTestData = useAppStore((s) => s.startTestData);
  const stopTestData = useAppStore((s) => s.stopTestData);

  const isTestData = transportConfig.kind === 'TestData';
  const isConnected = connectionState === 'Connected';

  const hasChannels = protocolConfig.kind === 'JustFloat' || protocolConfig.kind === 'FireWater';
  const isAuto = hasChannels && protocolConfig.channels == null;

  const updateKind = (kind: ProtocolConfig['kind']) => {
    if (kind === 'RawData' || kind === 'Slcan' || kind === 'CandleLight') {
      if (kind === 'Slcan') setProtocolConfig({ kind: 'Slcan' });
      else if (kind === 'CandleLight') setProtocolConfig({ kind: 'CandleLight' });
      else setProtocolConfig({ kind: 'RawData' });
    } else if (kind === 'JustFloat' || kind === 'FireWater') {
      const prevChannels = hasChannels ? protocolConfig.channels : null;
      setProtocolConfig({ kind, channels: prevChannels });
    } else if (kind === 'LogicDecode') {
      setProtocolConfig({
        kind: 'LogicDecode',
        decoder: {
          kind: 'Uart',
          params: { baud_rate: 115200, data_bits: 8, parity: 'none', stop_bits: 'one', channel: 0 },
        },
      });
    }
  };

  const setAutoMode = (auto: boolean) => {
    if (!hasChannels) return;
    setProtocolConfig({
      kind: protocolConfig.kind,
      channels: auto ? null : 4,
    });
  };

  const updateManualChannels = (channels: number) => {
    if (!hasChannels) return;
    const clamped = Math.max(1, Math.min(32, Math.floor(channels) || 1));
    setProtocolConfig({ kind: protocolConfig.kind, channels: clamped });
  };

  const switchDecoderKind = (decKind: LogicDecoderConfig['kind']) => {
    let decoder: LogicDecoderConfig;
    switch (decKind) {
      case 'Uart':
        decoder = {
          kind: 'Uart',
          params: { baud_rate: 115200, data_bits: 8, parity: 'none', stop_bits: 'one', channel: 0 },
        };
        break;
      case 'I2c':
        decoder = { kind: 'I2c', params: { sda_channel: 0, scl_channel: 1 } };
        break;
      case 'Spi':
        decoder = {
          kind: 'Spi',
          params: { sclk_channel: 0, mosi_channel: 1, miso_channel: 2, cs_channel: 3, mode: 0 },
        };
        break;
    }
    setProtocolConfig({ kind: 'LogicDecode', decoder });
  };

  const updateDecoderParams = <K extends LogicDecoderConfig['kind']>(
    decKind: K,
    patch: Partial<Extract<LogicDecoderConfig, { kind: K }>['params']>
  ) => {
    if (protocolConfig.kind !== 'LogicDecode') return;
    if (protocolConfig.decoder.kind !== decKind) return;
    const dec = protocolConfig.decoder as unknown as Extract<LogicDecoderConfig, { kind: K }>;
    const newDecoder = {
      kind: decKind,
      params: { ...dec.params, ...patch },
    } as unknown as LogicDecoderConfig;
    setProtocolConfig({ kind: 'LogicDecode', decoder: newDecoder });
  };

  const kinds: { value: ProtocolConfig['kind']; label: string }[] = [
    { value: 'JustFloat', label: t(lang, 'justfloat') },
    { value: 'FireWater', label: t(lang, 'firewater') },
    { value: 'RawData', label: t(lang, 'rawdata') },
    { value: 'Slcan', label: t(lang, 'slcan') },
    { value: 'CandleLight', label: t(lang, 'candleLight') },
    { value: 'LogicDecode', label: t(lang, 'logicAnalyzer') },
  ];

  const selectClass = 'w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded text-sm focus:outline-none focus:border-accent transition-colors';
  const inputClass = selectClass;

  return (
    <div>
      {/* TestData 开始/停止控制 */}
      {isTestData && (
        <div className="mb-3 p-2.5 bg-bg-input rounded border border-border">
          <div className="flex items-center justify-between mb-1.5">
            <span className="text-xs font-medium text-text-secondary">
              {t(lang, 'testData')}
            </span>
            <span className={`text-xs ${isConnected ? 'text-text-secondary' : 'text-text-disabled'}`}>
              {isConnected
                ? testDataRunning
                  ? t(lang, 'testDataRunning')
                  : t(lang, 'testDataStopped')
                : t(lang, 'notConnected')}
            </span>
          </div>
          {testDataRunning ? (
            <button
              type="button"
              onClick={() => stopTestData()}
              disabled={!isConnected}
              className="w-full px-3 py-1.5 bg-bg-danger text-text-bright border-none rounded cursor-pointer text-sm text-center transition-colors hover:bg-bg-danger-hover inline-flex items-center justify-center gap-1.5 disabled:opacity-50 disabled:cursor-default"
            >
              <Square size={14} />
              {t(lang, 'stopTestData')}
            </button>
          ) : (
            <button
              type="button"
              onClick={() => startTestData()}
              disabled={!isConnected}
              className="w-full px-3 py-1.5 bg-bg-button text-text-inverse border-none rounded cursor-pointer text-sm text-center transition-colors hover:bg-bg-button-hover inline-flex items-center justify-center gap-1.5 disabled:opacity-50 disabled:cursor-default"
            >
              <Play size={14} />
              {t(lang, 'startTestData')}
            </button>
          )}
        </div>
      )}

      {/* 协议类型 */}
      <div className="mb-2.5 mt-1">
        <label className="block text-xs text-text-secondary mb-1">{t(lang, 'protocolEngine')}</label>
        <select
          value={protocolConfig.kind}
          onChange={(e) => updateKind(e.target.value as ProtocolConfig['kind'])}
          className={selectClass}
        >
          {kinds.map((k) => (
            <option key={k.value} value={k.value}>{k.label}</option>
          ))}
        </select>
      </div>

      {/* 通道配置 (JustFloat / FireWater) */}
      {hasChannels && (
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
                className={inputClass}
              />
            </div>
          )}

          {isAuto && (
            <div className="mb-2.5 px-2 py-1.5 bg-bg-input rounded text-xs text-text-secondary flex justify-between items-center">
              <span>{t(lang, 'detectedChannels')}:</span>
              <span className="text-blue font-mono">
                {detectedChannels != null ? detectedChannels : '--'}
              </span>
            </div>
          )}
        </>
      )}

      {/* LogicDecode 解码器参数 */}
      {protocolConfig.kind === 'LogicDecode' && (
        <>
          <div className="mb-2.5">
            <label className="block text-xs text-text-secondary mb-1">{t(lang, 'decoderType')}</label>
            <select
              value={protocolConfig.decoder.kind}
              onChange={(e) => switchDecoderKind(e.target.value as LogicDecoderConfig['kind'])}
              className={selectClass}
            >
              <option value="Uart">{t(lang, 'uartConfig')}</option>
              <option value="I2c">{t(lang, 'i2cConfig')}</option>
              <option value="Spi">{t(lang, 'spiConfig')}</option>
            </select>
          </div>

          {protocolConfig.decoder.kind === 'Uart' && (
            <>
              <div className="mb-2.5">
                <label className="block text-xs text-text-secondary mb-1">{t(lang, 'baudRate')}</label>
                <select
                  value={protocolConfig.decoder.params.baud_rate}
                  onChange={(e) => updateDecoderParams('Uart', { baud_rate: parseInt(e.target.value) || 115200 })}
                  className={selectClass}
                >
                  {[9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600].map((b) => (
                    <option key={b} value={b}>{b}</option>
                  ))}
                </select>
              </div>
              <div className="flex gap-2">
                <div className="mb-2.5 flex-1">
                  <label className="block text-xs text-text-secondary mb-1">{t(lang, 'dataBits')}</label>
                  <select
                    value={protocolConfig.decoder.params.data_bits}
                    onChange={(e) => updateDecoderParams('Uart', { data_bits: parseInt(e.target.value) || 8 })}
                    className={selectClass}
                  >
                    {[5, 6, 7, 8].map((b) => (
                      <option key={b} value={b}>{b}</option>
                    ))}
                  </select>
                </div>
                <div className="mb-2.5 flex-1">
                  <label className="block text-xs text-text-secondary mb-1">{t(lang, 'parity')}</label>
                  <select
                    value={protocolConfig.decoder.params.parity}
                    onChange={(e) => updateDecoderParams('Uart', { parity: e.target.value as 'none' | 'odd' | 'even' })}
                    className={selectClass}
                  >
                    <option value="none">{t(lang, 'parityNone')}</option>
                    <option value="even">{t(lang, 'parityEven')}</option>
                    <option value="odd">{t(lang, 'parityOdd')}</option>
                  </select>
                </div>
              </div>
              <div className="flex gap-2">
                <div className="mb-2.5 flex-1">
                  <label className="block text-xs text-text-secondary mb-1">{t(lang, 'stopBits')}</label>
                  <select
                    value={protocolConfig.decoder.params.stop_bits}
                    onChange={(e) => updateDecoderParams('Uart', { stop_bits: e.target.value as 'one' | 'two' })}
                    className={selectClass}
                  >
                    <option value="one">{t(lang, 'stopBits1')}</option>
                    <option value="two">{t(lang, 'stopBits2')}</option>
                  </select>
                </div>
                <div className="mb-2.5 flex-1">
                  <label className="block text-xs text-text-secondary mb-1">{t(lang, 'channel')}</label>
                  <input
                    type="number"
                    min={0}
                    max={15}
                    value={protocolConfig.decoder.params.channel}
                    onChange={(e) => updateDecoderParams('Uart', { channel: parseInt(e.target.value) || 0 })}
                    className={inputClass}
                  />
                </div>
              </div>
            </>
          )}

          {protocolConfig.decoder.kind === 'I2c' && (
            <div className="flex gap-2">
              <div className="mb-2.5 flex-1">
                <label className="block text-xs text-text-secondary mb-1">{t(lang, 'sdaChannel')}</label>
                <input
                  type="number"
                  min={0}
                  max={15}
                  value={protocolConfig.decoder.params.sda_channel}
                  onChange={(e) => updateDecoderParams('I2c', { sda_channel: parseInt(e.target.value) || 0 })}
                  className={inputClass}
                />
              </div>
              <div className="mb-2.5 flex-1">
                <label className="block text-xs text-text-secondary mb-1">{t(lang, 'sclChannel')}</label>
                <input
                  type="number"
                  min={0}
                  max={15}
                  value={protocolConfig.decoder.params.scl_channel}
                  onChange={(e) => updateDecoderParams('I2c', { scl_channel: parseInt(e.target.value) || 0 })}
                  className={inputClass}
                />
              </div>
            </div>
          )}

          {protocolConfig.decoder.kind === 'Spi' && (
            <>
              <div className="flex gap-2">
                <div className="mb-2.5 flex-1">
                  <label className="block text-xs text-text-secondary mb-1">{t(lang, 'sclkChannel')}</label>
                  <input
                    type="number"
                    min={0}
                    max={15}
                    value={protocolConfig.decoder.params.sclk_channel}
                    onChange={(e) => updateDecoderParams('Spi', { sclk_channel: parseInt(e.target.value) || 0 })}
                    className={inputClass}
                  />
                </div>
                <div className="mb-2.5 flex-1">
                  <label className="block text-xs text-text-secondary mb-1">{t(lang, 'mosiChannel')}</label>
                  <input
                    type="number"
                    min={0}
                    max={15}
                    value={protocolConfig.decoder.params.mosi_channel}
                    onChange={(e) => updateDecoderParams('Spi', { mosi_channel: parseInt(e.target.value) || 0 })}
                    className={inputClass}
                  />
                </div>
              </div>
              <div className="flex gap-2">
                <div className="mb-2.5 flex-1">
                  <label className="block text-xs text-text-secondary mb-1">{t(lang, 'misoChannel')}</label>
                  <input
                    type="number"
                    min={0}
                    max={15}
                    value={protocolConfig.decoder.params.miso_channel}
                    onChange={(e) => updateDecoderParams('Spi', { miso_channel: parseInt(e.target.value) || 0 })}
                    className={inputClass}
                  />
                </div>
                <div className="mb-2.5 flex-1">
                  <label className="block text-xs text-text-secondary mb-1">{t(lang, 'csChannel')}</label>
                  <input
                    type="number"
                    min={0}
                    max={15}
                    value={protocolConfig.decoder.params.cs_channel}
                    onChange={(e) => updateDecoderParams('Spi', { cs_channel: parseInt(e.target.value) || 0 })}
                    className={inputClass}
                  />
                </div>
              </div>
              <div className="mb-2.5">
                <label className="block text-xs text-text-secondary mb-1">{t(lang, 'spiMode')}</label>
                <select
                  value={protocolConfig.decoder.params.mode}
                  onChange={(e) => updateDecoderParams('Spi', { mode: parseInt(e.target.value) || 0 })}
                  className={selectClass}
                >
                  {[0, 1, 2, 3].map((m) => (
                    <option key={m} value={m}>{m}</option>
                  ))}
                </select>
              </div>
            </>
          )}
        </>
      )}

      {/* 协议说明 */}
      <div className="mt-1 p-2 bg-bg-input rounded text-xs text-text-secondary leading-relaxed">
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
        {protocolConfig.kind === 'Slcan' && (
          <span className="inline-flex items-start gap-1.5">
            <Info size={14} className="flex-shrink-0 mt-0.25" />
            <span>{t(lang, 'slcanDesc')}</span>
          </span>
        )}
        {protocolConfig.kind === 'CandleLight' && (
          <span className="inline-flex items-start gap-1.5">
            <Info size={14} className="flex-shrink-0 mt-0.25" />
            <span>{t(lang, 'candleLightDesc')}</span>
          </span>
        )}
        {protocolConfig.kind === 'LogicDecode' && (
          <span className="inline-flex items-start gap-1.5">
            <Info size={14} className="flex-shrink-0 mt-0.25" />
            <span>
              {lang === 'zh'
                ? '逻辑分析仪解码, 支持 UART/I2C/SPI。需配合数字采样数据源。'
                : 'Logic analyzer decoder, supports UART/I2C/SPI. Requires digital sample source.'}
            </span>
          </span>
        )}
      </div>
    </div>
  );
}
