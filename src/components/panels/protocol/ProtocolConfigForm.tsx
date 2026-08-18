//! 受控协议配置表单 — 从旧 ProtocolSection 抽取, 供全局 Protocol 节点属性面板复用
//! (协议类型 / 通道配置 / LogicDecode 解码器参数 / 协议说明)
import { t, type Lang } from '../../../i18n';
import { Info } from 'lucide-react';
import type { ProtocolConfig, LogicDecoderConfig } from '../../../types';

interface ProtocolConfigFormProps {
  value: ProtocolConfig;
  onChange: (config: ProtocolConfig) => void;
  lang: Lang;
  /// 自动检测到的通道数 (仅自动模式有意义)
  detectedChannels?: number | null;
  /// 单选组名后缀 — 同一面板渲染多个表单时避免 radio name 冲突
  nameSuffix?: string;
}

export function ProtocolConfigForm({ value, onChange, lang, detectedChannels, nameSuffix = '' }: ProtocolConfigFormProps) {
  const hasChannels = value.kind === 'JustFloat' || value.kind === 'FireWater';
  const isAuto = hasChannels && value.channels == null;

  const updateKind = (kind: ProtocolConfig['kind']) => {
    if (kind === 'RawData' || kind === 'Slcan' || kind === 'CandleLight') {
      if (kind === 'Slcan') onChange({ kind: 'Slcan' });
      else if (kind === 'CandleLight') onChange({ kind: 'CandleLight' });
      else onChange({ kind: 'RawData' });
    } else if (kind === 'JustFloat' || kind === 'FireWater') {
      const prevChannels = hasChannels ? value.channels : null;
      onChange({ kind, channels: prevChannels });
    } else if (kind === 'LogicDecode') {
      onChange({
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
    onChange({ kind: value.kind, channels: auto ? null : 4 });
  };

  const updateManualChannels = (channels: number) => {
    if (!hasChannels) return;
    const clamped = Math.max(1, Math.min(32, Math.floor(channels) || 1));
    onChange({ kind: value.kind, channels: clamped });
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
    onChange({ kind: 'LogicDecode', decoder });
  };

  const updateDecoderParams = <K extends LogicDecoderConfig['kind']>(
    decKind: K,
    patch: Partial<Extract<LogicDecoderConfig, { kind: K }>['params']>
  ) => {
    if (value.kind !== 'LogicDecode') return;
    if (value.decoder.kind !== decKind) return;
    const dec = value.decoder as unknown as Extract<LogicDecoderConfig, { kind: K }>;
    const newDecoder = {
      kind: decKind,
      params: { ...dec.params, ...patch },
    } as unknown as LogicDecoderConfig;
    onChange({ kind: 'LogicDecode', decoder: newDecoder });
  };

  const kinds: { value: ProtocolConfig['kind']; label: string }[] = [
    { value: 'JustFloat', label: t(lang, 'justfloat') },
    { value: 'FireWater', label: t(lang, 'firewater') },
    { value: 'RawData', label: t(lang, 'rawdata') },
    { value: 'Slcan', label: t(lang, 'slcan') },
    { value: 'CandleLight', label: t(lang, 'candleLight') },
    { value: 'LogicDecode', label: t(lang, 'logicAnalyzer') },
  ];

  const selectClass = 'form-select';
  const inputClass = 'form-input';

  return (
    <div>
      {/* 协议类型 */}
      <div className="mb-2.5 mt-1">
        <label className="block text-xs text-text-secondary mb-1">{t(lang, 'protocolEngine')}</label>
        <select
          value={value.kind}
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
                  name={`channel-mode${nameSuffix}`}
                  checked={isAuto}
                  onChange={() => setAutoMode(true)}
                  className="accent-accent"
                />
                <span>{t(lang, 'channelsAuto')}</span>
              </label>
              <label className="flex items-center gap-1.5 cursor-pointer text-sm">
                <input
                  type="radio"
                  name={`channel-mode${nameSuffix}`}
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
                value={value.channels ?? 4}
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
      {value.kind === 'LogicDecode' && (
        <>
          <div className="mb-2.5">
            <label className="block text-xs text-text-secondary mb-1">{t(lang, 'decoderType')}</label>
            <select
              value={value.decoder.kind}
              onChange={(e) => switchDecoderKind(e.target.value as LogicDecoderConfig['kind'])}
              className={selectClass}
            >
              <option value="Uart">{t(lang, 'uartConfig')}</option>
              <option value="I2c">{t(lang, 'i2cConfig')}</option>
              <option value="Spi">{t(lang, 'spiConfig')}</option>
            </select>
          </div>

          {value.decoder.kind === 'Uart' && (
            <>
              <div className="mb-2.5">
                <label className="block text-xs text-text-secondary mb-1">{t(lang, 'baudRate')}</label>
                <select
                  value={value.decoder.params.baud_rate}
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
                    value={value.decoder.params.data_bits}
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
                    value={value.decoder.params.parity}
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
                    value={value.decoder.params.stop_bits}
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
                    value={value.decoder.params.channel}
                    onChange={(e) => updateDecoderParams('Uart', { channel: parseInt(e.target.value) || 0 })}
                    className={inputClass}
                  />
                </div>
              </div>
            </>
          )}

          {value.decoder.kind === 'I2c' && (
            <div className="flex gap-2">
              <div className="mb-2.5 flex-1">
                <label className="block text-xs text-text-secondary mb-1">{t(lang, 'sdaChannel')}</label>
                <input
                  type="number"
                  min={0}
                  max={15}
                  value={value.decoder.params.sda_channel}
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
                  value={value.decoder.params.scl_channel}
                  onChange={(e) => updateDecoderParams('I2c', { scl_channel: parseInt(e.target.value) || 0 })}
                  className={inputClass}
                />
              </div>
            </div>
          )}

          {value.decoder.kind === 'Spi' && (
            <>
              <div className="flex gap-2">
                <div className="mb-2.5 flex-1">
                  <label className="block text-xs text-text-secondary mb-1">{t(lang, 'sclkChannel')}</label>
                  <input
                    type="number"
                    min={0}
                    max={15}
                    value={value.decoder.params.sclk_channel}
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
                    value={value.decoder.params.mosi_channel}
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
                    value={value.decoder.params.miso_channel}
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
                    value={value.decoder.params.cs_channel}
                    onChange={(e) => updateDecoderParams('Spi', { cs_channel: parseInt(e.target.value) || 0 })}
                    className={inputClass}
                  />
                </div>
              </div>
              <div className="mb-2.5">
                <label className="block text-xs text-text-secondary mb-1">{t(lang, 'spiMode')}</label>
                <select
                  value={value.decoder.params.mode}
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
        {value.kind === 'JustFloat' && (
          <>
            <strong className="text-text-primary">JustFloat</strong>
            <br />
            {lang === 'zh'
              ? '4 字节小端浮点数 + 帧尾 [0x00,0x00,0x80,0x7f]。适合高速波形传输。'
              : '4-byte LE floats + tail [0x00,0x00,0x80,0x7f]. High-throughput waveform.'}
          </>
        )}
        {value.kind === 'FireWater' && (
          <>
            <strong className="text-text-primary">FireWater</strong>
            <br />
            {lang === 'zh'
              ? 'CSV 格式, 通道间逗号分隔, 以 \\n 结尾。可读性强。'
              : 'CSV format, channels separated by commas, ends with \\n. Human-readable.'}
          </>
        )}
        {value.kind === 'RawData' && (
          <>
            <strong className="text-text-primary">RawData</strong>
            <br />
            {lang === 'zh'
              ? '原始字节流, 不解析。仅显示原始数据。'
              : 'Raw byte stream, no parsing. Raw data only.'}
          </>
        )}
        {value.kind === 'Slcan' && (
          <span className="inline-flex items-start gap-1.5">
            <Info size={14} className="flex-shrink-0 mt-0.25" />
            <span>{t(lang, 'slcanDesc')}</span>
          </span>
        )}
        {value.kind === 'CandleLight' && (
          <span className="inline-flex items-start gap-1.5">
            <Info size={14} className="flex-shrink-0 mt-0.25" />
            <span>{t(lang, 'candleLightDesc')}</span>
          </span>
        )}
        {value.kind === 'LogicDecode' && (
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
