//! 受控传输配置表单 — 从旧 TransportConfigPanel 抽取, 供全局 Transport 节点属性面板复用
import { useState, useEffect, useCallback } from 'react';
import { t, type Lang } from '../../../i18n';
import { listCandleDevices } from '../../../lib/buffers/canSubscription';
import { defaultTransportConfig } from '../../../store/appStoreHelpers';
import { SerialForm, UdpForm, TcpClientForm, TcpServerForm, TestDataForm, SlcanForm, CandleForm } from '.';
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
} from '../../../types';

interface TransportConfigFormProps {
  value: TransportConfig;
  onChange: (config: TransportConfig) => void;
  lang: Lang;
  /// TestData 表单提示用 — 关联协议引擎显示名
  protocolLabel?: string;
}

/// 数据接口配置表单 (类型选择 + 参数) — 受控组件, 不含连接按钮
export function TransportConfigForm({ value, onChange, lang, protocolLabel }: TransportConfigFormProps) {
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
    if (value.kind === 'CandleLight' && candleDevices.length === 0) {
      void refreshCandleDevices();
    }
  }, [value.kind, candleDevices.length, refreshCandleDevices]);

  const kinds: { value: TransportConfig['kind']; label: string }[] = [
    { value: 'Serial', label: t(lang, 'serial') },
    { value: 'Udp', label: t(lang, 'udp') },
    { value: 'TcpClient', label: t(lang, 'tcpClient') },
    { value: 'TcpServer', label: t(lang, 'tcpServer') },
    { value: 'TestData', label: t(lang, 'testData') },
    { value: 'Slcan', label: t(lang, 'slcan') },
    { value: 'CandleLight', label: t(lang, 'candleLight') },
  ];

  const updateSerial = <K extends keyof SerialConfig>(key: K, val: SerialConfig[K]) => {
    if (value.kind !== 'Serial') return;
    onChange({ kind: 'Serial', params: { ...value.params, [key]: val } });
  };
  const updateUdp = (patch: Partial<UdpConfig>) => {
    if (value.kind !== 'Udp') return;
    onChange({ kind: 'Udp', params: { ...value.params, ...patch } });
  };
  const updateTcpClient = (patch: Partial<TcpClientConfig>) => {
    if (value.kind !== 'TcpClient') return;
    onChange({ kind: 'TcpClient', params: { ...value.params, ...patch } });
  };
  const updateTcpServer = (patch: Partial<TcpServerConfig>) => {
    if (value.kind !== 'TcpServer') return;
    onChange({ kind: 'TcpServer', params: { ...value.params, ...patch } });
  };
  const updateTestData = (patch: Partial<TestDataConfig>) => {
    if (value.kind !== 'TestData') return;
    onChange({ kind: 'TestData', params: { ...value.params, ...patch } });
  };
  const updateSlcan = (patch: Partial<SlcanConfig>) => {
    if (value.kind !== 'Slcan') return;
    onChange({ kind: 'Slcan', params: { ...value.params, ...patch } });
  };
  const updateCandle = (patch: Partial<CandleConfig>) => {
    if (value.kind !== 'CandleLight') return;
    onChange({ kind: 'CandleLight', params: { ...value.params, ...patch } });
  };

  return (
    <div>
      {/* 数据接口类型选择器 */}
      <div className="mb-2.5">
        <label className="block text-xs text-text-secondary mb-1">{t(lang, 'transportType')}</label>
        <select
          value={value.kind}
          onChange={(e) => onChange(defaultTransportConfig(e.target.value as TransportConfig['kind']))}
          className="form-select"
        >
          {kinds.map((k) => (
            <option key={k.value} value={k.value}>{k.label}</option>
          ))}
        </select>
      </div>

      {value.kind === 'Serial' && (
        <SerialForm params={value.params} onChange={updateSerial} lang={lang} />
      )}
      {value.kind === 'Udp' && (
        <UdpForm params={value.params} onChange={updateUdp} lang={lang} />
      )}
      {value.kind === 'TcpClient' && (
        <TcpClientForm params={value.params} onChange={updateTcpClient} lang={lang} />
      )}
      {value.kind === 'TcpServer' && (
        <TcpServerForm params={value.params} onChange={updateTcpServer} lang={lang} />
      )}
      {value.kind === 'TestData' && (
        <TestDataForm params={value.params} onChange={updateTestData} lang={lang} protocolLabel={protocolLabel} />
      )}
      {value.kind === 'Slcan' && (
        <SlcanForm params={value.params} onChange={updateSlcan} lang={lang} />
      )}
      {value.kind === 'CandleLight' && (
        <CandleForm
          params={value.params}
          onChange={updateCandle}
          lang={lang}
          candleDevices={candleDevices}
          candleLoading={candleLoading}
          refreshCandleDevices={refreshCandleDevices}
        />
      )}
    </div>
  );
}
