import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { Plug, PlugZap } from 'lucide-react';
import { useState, useEffect, useCallback } from 'react';
import { listCandleDevices } from '../../lib/canSubscription';
import { SerialForm, UdpForm, TcpClientForm, TcpServerForm, TestDataForm, SlcanForm, CandleForm } from './transport';
import type {
  TransportConfig,
  UdpConfig,
  TcpClientConfig,
  TcpServerConfig,
  TestDataConfig,
  SlcanConfig,
  CandleConfig,
  CandleDeviceInfo,
  SerialConfig,
} from '../../types';

/// 数据接口面板 — 仅包含传输类型 + 设备参数 + 连接控制
export function TransportConfigPanel() {
  const lang = useAppStore((s) => s.lang);
  const transportConfig = useAppStore((s) => s.transportConfig);
  const setTransportConfig = useAppStore((s) => s.setTransportConfig);
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

  const isConnected = connectionState === 'Connected';
  const selectClass = 'form-select';

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
        <SerialForm params={transportConfig.params} onChange={updateSerial} lang={lang} />
      )}

      {/* === Udp === */}
      {transportConfig.kind === 'Udp' && (
        <UdpForm params={transportConfig.params} onChange={updateUdp} lang={lang} />
      )}

      {/* === TcpClient === */}
      {transportConfig.kind === 'TcpClient' && (
        <TcpClientForm params={transportConfig.params} onChange={updateTcpClient} lang={lang} />
      )}

      {/* === TcpServer === */}
      {transportConfig.kind === 'TcpServer' && (
        <TcpServerForm params={transportConfig.params} onChange={updateTcpServer} lang={lang} />
      )}

      {/* === TestData === */}
      {transportConfig.kind === 'TestData' && (
        <TestDataForm params={transportConfig.params} onChange={updateTestData} lang={lang} />
      )}

      {/* === Slcan === */}
      {transportConfig.kind === 'Slcan' && (
        <SlcanForm params={transportConfig.params} onChange={updateSlcan} lang={lang} />
      )}

      {/* === CandleLight === */}
      {transportConfig.kind === 'CandleLight' && (
        <CandleForm
          params={transportConfig.params}
          onChange={updateCandle}
          lang={lang}
          candleDevices={candleDevices}
          candleLoading={candleLoading}
          refreshCandleDevices={refreshCandleDevices}
        />
      )}

      {/* 连接控制 */}
      <div className="mt-3 pt-2 border-t border-border" data-tour="connect">
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
            className="w-full px-3 py-1.5 bg-bg-button text-text-inverse border-none rounded cursor-pointer text-sm text-center transition-colors hover:bg-bg-button-hover inline-flex items-center justify-center gap-1.5 disabled:opacity-50 disabled:cursor-default"
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
