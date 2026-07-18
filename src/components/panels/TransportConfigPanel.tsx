import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { Info, RefreshCw, Plug, PlugZap } from 'lucide-react';
import { useState, useEffect, useCallback } from 'react';
import { listCandleDevices } from '../../lib/canSubscription';
import { PortPicker } from './PortPicker';
import type {
  TransportConfig,
  UdpConfig,
  TcpClientConfig,
  TcpServerConfig,
  TestDataConfig,
  SlcanConfig,
  CandleConfig,
  CanBitrate,
  CandleDeviceInfo,
  SerialConfig,
} from '../../types';

/// 数据接口面板 — 仅包含传输类型 + 设备参数 + 连接控制
export function TransportConfigPanel() {
  const lang = useAppStore((s) => s.lang);
  const transportConfig = useAppStore((s) => s.transportConfig);
  const setTransportConfig = useAppStore((s) => s.setTransportConfig);
  const protocolConfig = useAppStore((s) => s.protocolConfig);
  const connectionState = useAppStore((s) => s.connectionState);
  const connect = useAppStore((s) => s.connect);
  const disconnect = useAppStore((s) => s.disconnect);

  const [candleDevices, setCandleDevices] = useState<CandleDeviceInfo[]>([]);
  const [candleLoading, setCandleLoading] = useState(false);

  const refreshCandleDevices = useCallback(async () => {
    setCandleLoading(true);
    try {
      const list = await listCandleDevices();
      setCandleDevices(list);
    } catch {
      setCandleDevices([]);
    } finally {
      setCandleLoading(false);
    }
  }, []);

  useEffect(() => {
    if (transportConfig.kind === 'CandleLight' && candleDevices.length === 0) {
      void refreshCandleDevices();
    }
  }, [transportConfig.kind, candleDevices.length, refreshCandleDevices]);

  const kinds: { value: TransportConfig['kind']; label: string }[] = [
    { value: 'Serial', label: t(lang, 'serial') },
    { value: 'Udp', label: t(lang, 'udp') },
    { value: 'TcpClient', label: t(lang, 'tcpClient') },
    { value: 'TcpServer', label: t(lang, 'tcpServer') },
    { value: 'TestData', label: t(lang, 'testData') },
    { value: 'Slcan', label: t(lang, 'slcan') },
    { value: 'CandleLight', label: t(lang, 'candleLight') },
  ];

  const switchKind = (kind: TransportConfig['kind']) => {
    let defaults: TransportConfig;
    switch (kind) {
      case 'Serial':
        defaults = {
          kind: 'Serial',
          params: {
            port_name: '',
            baud_rate: 115200,
            data_bits: 8,
            parity: 'none',
            stop_bits: 'one',
            flow_control: 'none',
          },
        };
        break;
      case 'Udp':
        defaults = {
          kind: 'Udp',
          params: {
            local_addr: '0.0.0.0',
            remote_addr: '127.0.0.1',
            local_port: 8888,
            remote_port: 9999,
          },
        };
        break;
      case 'TcpClient':
        defaults = {
          kind: 'TcpClient',
          params: { host: '127.0.0.1', port: 8080 },
        };
        break;
      case 'TcpServer':
        defaults = {
          kind: 'TcpServer',
          params: { listen_addr: '0.0.0.0', listen_port: 8080 },
        };
        break;
      case 'TestData':
        defaults = {
          kind: 'TestData',
          params: { channels: 4, sample_rate: 100, signal: 'Sine' },
        };
        break;
      case 'Slcan':
        defaults = {
          kind: 'Slcan',
          params: { port_name: '', baud_rate: 115200, can_bitrate: 'bps500k' },
        };
        break;
      case 'CandleLight':
        defaults = {
          kind: 'CandleLight',
          params: { bus: 1, address: 0, can_bitrate: 'bps500k', channel: 0 },
        };
        break;
    }
    setTransportConfig(defaults);
  };

  const updateSerial = <K extends keyof SerialConfig>(
    key: K,
    value: SerialConfig[K]
  ) => {
    if (transportConfig.kind !== 'Serial') return;
    setTransportConfig({
      kind: 'Serial',
      params: { ...transportConfig.params, [key]: value },
    });
  };

  const updateUdp = (patch: Partial<UdpConfig>) => {
    if (transportConfig.kind !== 'Udp') return;
    setTransportConfig({ kind: 'Udp', params: { ...transportConfig.params, ...patch } });
  };

  const updateTcpClient = (patch: Partial<TcpClientConfig>) => {
    if (transportConfig.kind !== 'TcpClient') return;
    setTransportConfig({ kind: 'TcpClient', params: { ...transportConfig.params, ...patch } });
  };

  const updateTcpServer = (patch: Partial<TcpServerConfig>) => {
    if (transportConfig.kind !== 'TcpServer') return;
    setTransportConfig({ kind: 'TcpServer', params: { ...transportConfig.params, ...patch } });
  };

  const updateTestData = (patch: Partial<TestDataConfig>) => {
    if (transportConfig.kind !== 'TestData') return;
    setTransportConfig({ kind: 'TestData', params: { ...transportConfig.params, ...patch } });
  };

  const updateSlcan = (patch: Partial<SlcanConfig>) => {
    if (transportConfig.kind !== 'Slcan') return;
    setTransportConfig({ kind: 'Slcan', params: { ...transportConfig.params, ...patch } });
  };

  const updateCandle = (patch: Partial<CandleConfig>) => {
    if (transportConfig.kind !== 'CandleLight') return;
    setTransportConfig({ kind: 'CandleLight', params: { ...transportConfig.params, ...patch } });
  };

  const signalOptions: { value: TestDataConfig['signal']; label: string }[] = [
    { value: 'Sine', label: t(lang, 'sine') },
    { value: 'Square', label: t(lang, 'square') },
    { value: 'Triangle', label: t(lang, 'triangle') },
    { value: 'Sawtooth', label: t(lang, 'sawtooth') },
    { value: 'Random', label: t(lang, 'random') },
    { value: 'Dc', label: t(lang, 'dc') },
    { value: 'Chirp', label: t(lang, 'chirp') },
    { value: 'Steps', label: t(lang, 'steps') },
    { value: 'Noise', label: t(lang, 'noise') },
    { value: 'MultiTone', label: t(lang, 'multitone') },
  ];

  const slcanBaudOptions = [9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600];
  const canBitrateOptions: { value: CanBitrate; label: string }[] = [
    { value: 'bps100k', label: '100k' },
    { value: 'bps125k', label: '125k' },
    { value: 'bps250k', label: '250k' },
    { value: 'bps500k', label: '500k' },
    { value: 'bps1m', label: '1M' },
  ];
  const baudRates = [9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600];

  const isConnected = connectionState === 'Connected';
  const protocolLabel: string = (() => {
    switch (protocolConfig.kind) {
      case 'JustFloat': return 'JustFloat';
      case 'FireWater': return 'FireWater';
      case 'RawData': return 'RawData';
      case 'Slcan': return 'Slcan';
      case 'CandleLight': return 'CandleLight';
      case 'LogicDecode': return 'LogicDecode';
    }
  })();

  const inputClass = 'w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded text-sm focus:outline-none focus:border-accent transition-colors';
  const selectClass = inputClass;

  return (
    <div>
      {/* 数据接口类型选择器 */}
      <div className="mb-2.5">
        <label className="block text-xs text-text-secondary mb-1">{t(lang, 'transportType')}</label>
        <select
          value={transportConfig.kind}
          onChange={(e) => switchKind(e.target.value as TransportConfig['kind'])}
          className={selectClass}
        >
          {kinds.map((k) => (
            <option key={k.value} value={k.value}>{k.label}</option>
          ))}
        </select>
      </div>

      {/* === Serial: 端口列表 + 串口参数 === */}
      {transportConfig.kind === 'Serial' && (
        <>
          <PortPicker />
          <div className="mb-2.5">
            <label className="block text-xs text-text-secondary mb-1">{t(lang, 'baudRate')}</label>
            <select
              className={selectClass}
              value={transportConfig.params.baud_rate}
              onChange={(e) => updateSerial('baud_rate', parseInt(e.target.value))}
            >
              {baudRates.map((rate) => (
                <option key={rate} value={rate}>{rate}</option>
              ))}
            </select>
          </div>
          <div className="flex gap-2">
            <div className="mb-2.5 flex-1">
              <label className="block text-xs text-text-secondary mb-1">{t(lang, 'dataBits')}</label>
              <select
                className={selectClass}
                value={transportConfig.params.data_bits}
                onChange={(e) => updateSerial('data_bits', parseInt(e.target.value))}
              >
                {[5, 6, 7, 8].map((b) => <option key={b} value={b}>{b}</option>)}
              </select>
            </div>
            <div className="mb-2.5 flex-1">
              <label className="block text-xs text-text-secondary mb-1">{t(lang, 'parity')}</label>
              <select
                className={selectClass}
                value={transportConfig.params.parity}
                onChange={(e) => updateSerial('parity', e.target.value as SerialConfig['parity'])}
              >
                <option value="none">None</option>
                <option value="odd">Odd</option>
                <option value="even">Even</option>
              </select>
            </div>
          </div>
          <div className="flex gap-2">
            <div className="mb-2.5 flex-1">
              <label className="block text-xs text-text-secondary mb-1">{t(lang, 'stopBits')}</label>
              <select
                className={selectClass}
                value={transportConfig.params.stop_bits}
                onChange={(e) => updateSerial('stop_bits', e.target.value as SerialConfig['stop_bits'])}
              >
                <option value="one">1</option>
                <option value="two">2</option>
              </select>
            </div>
            <div className="mb-2.5 flex-1">
              <label className="block text-xs text-text-secondary mb-1">{t(lang, 'flowControl')}</label>
              <select
                className={selectClass}
                value={transportConfig.params.flow_control}
                onChange={(e) => updateSerial('flow_control', e.target.value as SerialConfig['flow_control'])}
              >
                <option value="none">None</option>
                <option value="software">Software</option>
                <option value="hardware">Hardware</option>
              </select>
            </div>
          </div>
        </>
      )}

      {/* === Udp === */}
      {transportConfig.kind === 'Udp' && (
        <>
          <div className="flex gap-2">
            <div className="mb-2.5 flex-1">
              <label className="block text-xs text-text-secondary mb-1">{t(lang, 'localAddr')}</label>
              <input
                type="text"
                value={transportConfig.params.local_addr}
                onChange={(e) => updateUdp({ local_addr: e.target.value })}
                className={inputClass}
              />
            </div>
            <div className="mb-2.5 w-20">
              <label className="block text-xs text-text-secondary mb-1">{t(lang, 'localPort')}</label>
              <input
                type="number"
                value={transportConfig.params.local_port}
                onChange={(e) => updateUdp({ local_port: parseInt(e.target.value) || 0 })}
                className={inputClass}
              />
            </div>
          </div>
          <div className="flex gap-2">
            <div className="mb-2.5 flex-1">
              <label className="block text-xs text-text-secondary mb-1">{t(lang, 'remoteAddr')}</label>
              <input
                type="text"
                value={transportConfig.params.remote_addr}
                onChange={(e) => updateUdp({ remote_addr: e.target.value })}
                className={inputClass}
              />
            </div>
            <div className="mb-2.5 w-20">
              <label className="block text-xs text-text-secondary mb-1">{t(lang, 'remotePort')}</label>
              <input
                type="number"
                value={transportConfig.params.remote_port}
                onChange={(e) => updateUdp({ remote_port: parseInt(e.target.value) || 0 })}
                className={inputClass}
              />
            </div>
          </div>
        </>
      )}

      {/* === TcpClient === */}
      {transportConfig.kind === 'TcpClient' && (
        <div className="flex gap-2">
          <div className="mb-2.5 flex-1">
            <label className="block text-xs text-text-secondary mb-1">{t(lang, 'host')}</label>
            <input
              type="text"
              value={transportConfig.params.host}
              onChange={(e) => updateTcpClient({ host: e.target.value })}
              className={inputClass}
            />
          </div>
          <div className="mb-2.5 w-20">
            <label className="block text-xs text-text-secondary mb-1">{t(lang, 'port')}</label>
            <input
              type="number"
              value={transportConfig.params.port}
              onChange={(e) => updateTcpClient({ port: parseInt(e.target.value) || 0 })}
              className={inputClass}
            />
          </div>
        </div>
      )}

      {/* === TcpServer === */}
      {transportConfig.kind === 'TcpServer' && (
        <div className="flex gap-2">
          <div className="mb-2.5 flex-1">
            <label className="block text-xs text-text-secondary mb-1">{t(lang, 'listenAddr')}</label>
            <input
              type="text"
              value={transportConfig.params.listen_addr}
              onChange={(e) => updateTcpServer({ listen_addr: e.target.value })}
              className={inputClass}
            />
          </div>
          <div className="mb-2.5 w-20">
            <label className="block text-xs text-text-secondary mb-1">{t(lang, 'listenPort')}</label>
            <input
              type="number"
              value={transportConfig.params.listen_port}
              onChange={(e) => updateTcpServer({ listen_port: parseInt(e.target.value) || 0 })}
              className={inputClass}
            />
          </div>
        </div>
      )}

      {/* === TestData === */}
      {transportConfig.kind === 'TestData' && (
        <>
          <div className="flex gap-2">
            <div className="mb-2.5 flex-1">
              <label className="block text-xs text-text-secondary mb-1">{t(lang, 'channels')}</label>
              <input
                type="number"
                min={1}
                max={32}
                value={transportConfig.params.channels}
                onChange={(e) => updateTestData({ channels: parseInt(e.target.value) || 1 })}
                className={inputClass}
              />
            </div>
            <div className="mb-2.5 flex-1">
              <label className="block text-xs text-text-secondary mb-1">{t(lang, 'sampleRate')}</label>
              <input
                type="number"
                min={1}
                max={10000}
                value={transportConfig.params.sample_rate}
                onChange={(e) => updateTestData({ sample_rate: parseInt(e.target.value) || 1 })}
                className={inputClass}
              />
            </div>
          </div>
          <div className="mb-2.5">
            <label className="block text-xs text-text-secondary mb-1">{t(lang, 'signalType')}</label>
            <select
              value={transportConfig.params.signal}
              onChange={(e) => updateTestData({ signal: e.target.value as TestDataConfig['signal'] })}
              className={selectClass}
            >
              {signalOptions.map((opt) => (
                <option key={opt.value} value={opt.value}>{opt.label}</option>
              ))}
            </select>
          </div>
          <div className="p-2 bg-bg-input rounded text-xs text-text-secondary leading-relaxed flex gap-2 mb-1">
            <Info size={14} className="flex-shrink-0 mt-0.25" />
            <span>
              {lang === 'zh'
                ? `测试数据将根据当前协议引擎 (${protocolLabel}) 自动适配数据格式。`
                : `Test data will be auto-formatted for the current protocol engine (${protocolLabel}).`}
            </span>
          </div>
        </>
      )}

      {/* === Slcan === */}
      {transportConfig.kind === 'Slcan' && (
        <>
          <PortPicker />
          <div className="flex gap-2">
            <div className="mb-2.5 flex-1">
              <label className="block text-xs text-text-secondary mb-1">{t(lang, 'baudRate')}</label>
              <select
                value={transportConfig.params.baud_rate}
                onChange={(e) => updateSlcan({ baud_rate: parseInt(e.target.value) || 115200 })}
                className={selectClass}
              >
                {slcanBaudOptions.map((b) => <option key={b} value={b}>{b}</option>)}
              </select>
            </div>
            <div className="mb-2.5 flex-1">
              <label className="block text-xs text-text-secondary mb-1">{t(lang, 'canBitrate')}</label>
              <select
                value={transportConfig.params.can_bitrate}
                onChange={(e) => updateSlcan({ can_bitrate: e.target.value as CanBitrate })}
                className={selectClass}
              >
                {canBitrateOptions.map((opt) => (
                  <option key={opt.value} value={opt.value}>{opt.label}</option>
                ))}
              </select>
            </div>
          </div>
          <div className="p-2 bg-bg-input rounded text-xs text-text-secondary leading-relaxed flex gap-2 mb-1">
            <Info size={14} className="flex-shrink-0 mt-0.25" />
            <span>{t(lang, 'slcanDesc')}</span>
          </div>
        </>
      )}

      {/* === CandleLight === */}
      {transportConfig.kind === 'CandleLight' && (
        <>
          <div className="mb-2.5">
            <div className="flex items-center justify-between mb-1">
              <label className="block text-xs text-text-secondary">{t(lang, 'candleDevice')}</label>
              <button
                type="button"
                onClick={() => void refreshCandleDevices()}
                disabled={candleLoading}
                className="text-text-secondary hover:text-text-primary transition-colors disabled:opacity-50 cursor-pointer"
                title={t(lang, 'refresh')}
              >
                <RefreshCw size={12} className={candleLoading ? 'animate-spin' : ''} />
              </button>
            </div>
            <select
              value={`${transportConfig.params.bus}:${transportConfig.params.address}`}
              onChange={(e) => {
                const sel = e.target.value;
                const dev = candleDevices.find((d) => `${d.bus}:${d.address}` === sel);
                if (dev) {
                  updateCandle({ bus: dev.bus, address: dev.address });
                }
              }}
              className={selectClass}
            >
              {candleDevices.length === 0 && (
                <option value="">-- {t(lang, 'noCandleDevices')} --</option>
              )}
              {candleDevices.map((d) => (
                <option key={`${d.bus}:${d.address}`} value={`${d.bus}:${d.address}`}>
                  Bus {d.bus}:Dev {d.address} ({d.vid.toString(16).padStart(4, '0').toUpperCase()}:{d.pid.toString(16).padStart(4, '0').toUpperCase()})
                  {d.product ? ` - ${d.product}` : ''}
                </option>
              ))}
            </select>
          </div>
          <div className="flex gap-2">
            <div className="mb-2.5 flex-1">
              <label className="block text-xs text-text-secondary mb-1">{t(lang, 'channel')}</label>
              <select
                value={transportConfig.params.channel}
                onChange={(e) => updateCandle({ channel: parseInt(e.target.value) || 0 })}
                className={selectClass}
              >
                <option value={0}>0</option>
                <option value={1}>1</option>
              </select>
            </div>
            <div className="mb-2.5 flex-1">
              <label className="block text-xs text-text-secondary mb-1">{t(lang, 'canBitrate')}</label>
              <select
                value={transportConfig.params.can_bitrate}
                onChange={(e) => updateCandle({ can_bitrate: e.target.value as CanBitrate })}
                className={selectClass}
              >
                {canBitrateOptions.map((opt) => (
                  <option key={opt.value} value={opt.value}>{opt.label}</option>
                ))}
              </select>
            </div>
          </div>
          <div className="p-2 bg-bg-input rounded text-xs text-text-secondary leading-relaxed flex gap-2 mb-1">
            <Info size={14} className="flex-shrink-0 mt-0.25" />
            <span>{t(lang, 'candleLightDesc')}</span>
          </div>
        </>
      )}

      {/* 连接控制 */}
      <div className="mt-3 pt-2 border-t border-border">
        {isConnected ? (
          <button
            className="w-full px-3 py-1.5 bg-bg-danger text-text-bright border-none rounded cursor-pointer text-sm text-center transition-colors hover:bg-bg-danger-hover inline-flex items-center justify-center gap-1.5"
            onClick={() => disconnect()}
          >
            <PlugZap size={14} />
            {t(lang, 'disconnect')}
          </button>
        ) : (
          <button
            className="w-full px-3 py-1.5 bg-bg-button text-text-bright border-none rounded cursor-pointer text-sm text-center transition-colors hover:bg-bg-button-hover inline-flex items-center justify-center gap-1.5 disabled:opacity-50 disabled:cursor-default"
            onClick={() => connect()}
            disabled={connectionState === 'Connecting'}
          >
            <Plug size={14} />
            {t(lang, 'connect')}
          </button>
        )}
      </div>
    </div>
  );
}
