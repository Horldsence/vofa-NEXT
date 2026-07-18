import { useState, useMemo } from 'react';
import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { RefreshCw, Usb, Plug, PlugZap, Search, Cpu } from 'lucide-react';
import type { SerialConfig } from '../../types';

/// 串口配置面板 — 端口列表 + 筛选 + 参数配置 + 连接按钮
export function PortConfig() {
  const lang = useAppStore((s) => s.lang);
  const ports = useAppStore((s) => s.ports);
  const selectedPortIndex = useAppStore((s) => s.selectedPortIndex);
  const transportConfig = useAppStore((s) => s.transportConfig);
  const connectionState = useAppStore((s) => s.connectionState);
  const refreshPorts = useAppStore((s) => s.refreshPorts);
  const selectPort = useAppStore((s) => s.selectPort);
  const setTransportConfig = useAppStore((s) => s.setTransportConfig);
  const connect = useAppStore((s) => s.connect);
  const disconnect = useAppStore((s) => s.disconnect);

  const [filter, setFilter] = useState('');

  const isSerial = transportConfig.kind === 'Serial';
  const params = isSerial ? (transportConfig.params as SerialConfig) : null;
  const isConnected = connectionState === 'Connected';

  // 筛选端口: 按名称 / 产品 / 厂商 / VID:PID 匹配
  const filteredPorts = useMemo(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return ports.map((p, idx) => ({ port: p, idx }));
    return ports
      .map((p, idx) => ({ port: p, idx }))
      .filter(({ port }) => {
        const vidPid = `${port.vid ?? ''}:${port.pid ?? ''}`;
        return (
          port.name.toLowerCase().includes(q) ||
          (port.product ?? '').toLowerCase().includes(q) ||
          (port.manufacturer ?? '').toLowerCase().includes(q) ||
          vidPid.includes(q)
        );
      });
  }, [ports, filter]);

  const updateParam = <K extends keyof SerialConfig>(
    key: K,
    value: SerialConfig[K]
  ) => {
    if (!isSerial || !params) return;
    setTransportConfig({
      kind: 'Serial',
      params: { ...params, [key]: value },
    });
  };

  const baudRates = [9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600];

  return (
    <div className="p-3">
      <div className="mb-2.5">
        <div className="flex gap-2 items-center mb-1.5">
          <span className="block text-xs text-text-secondary m-0 flex-1">
            {t(lang, 'portName')}
          </span>
          <button
            className="w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors cursor-pointer"
            title={t(lang, 'refresh')}
            onClick={() => refreshPorts()}
          >
            <RefreshCw size={14} />
          </button>
        </div>
        <div className="flex items-center gap-1.5 bg-bg-input border border-border rounded px-2 mb-1.5 text-text-secondary">
          <Search size={12} />
          <input
            type="text"
            className="bg-transparent border-none py-1 flex-1 focus:outline-none text-text-primary text-sm"
            placeholder={lang === 'zh' ? '筛选端口...' : 'Filter ports...'}
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
        </div>
        <div className="flex flex-col gap-0.5">
          {ports.length === 0 ? (
            <div className="p-3 text-text-secondary text-xs text-center">
              {lang === 'zh' ? '未发现串口' : 'No ports found'}
            </div>
          ) : filteredPorts.length === 0 ? (
            <div className="p-3 text-text-secondary text-xs text-center">
              {lang === 'zh' ? '无匹配端口' : 'No matching ports'}
            </div>
          ) : (
            filteredPorts.map(({ port, idx }) => (
              <div
                key={port.name}
                className={`px-2 py-1.5 rounded cursor-pointer flex flex-col gap-0.5 transition-colors hover:bg-bg-hover ${idx === selectedPortIndex ? 'bg-bg-active' : ''}`}
                onClick={() => selectPort(idx)}
              >
                <div className="text-sm text-text-primary font-mono">
                  <Usb size={10} className="inline mr-1" />
                  {port.name}
                </div>
                {(port.product || port.manufacturer) && (
                  <div className="text-[10px] text-text-secondary">
                    <Cpu size={9} className="inline mr-0.5 align-middle" />
                    {[port.product, port.manufacturer]
                      .filter(Boolean)
                      .join(' • ')}
                  </div>
                )}
                {(port.vid !== null || port.pid !== null) && (
                  <div className="text-[10px] font-mono text-blue">
                    VID:{port.vid !== null ? port.vid.toString(16).toUpperCase().padStart(4, '0') : '----'}
                    {'  PID:'}
                    {port.pid !== null ? port.pid.toString(16).toUpperCase().padStart(4, '0') : '----'}
                    {'  ('}
                    {port.port_type}
                    {')'}
                  </div>
                )}
                {port.serial_number && (
                  <div className="text-[10px] font-mono text-text-secondary">
                    S/N: {port.serial_number}
                  </div>
                )}
              </div>
            ))
          )}
        </div>
      </div>

      {isSerial && params && (
        <>
          <div className="mb-2.5">
            <label className="block text-xs text-text-secondary mb-1">{t(lang, 'baudRate')}</label>
            <select
              className="w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded text-sm focus:outline-none focus:border-accent transition-colors appearance-none bg-no-repeat bg-right-6 bg-center pr-6 cursor-pointer"
              value={params.baud_rate}
              onChange={(e) =>
                updateParam('baud_rate', parseInt(e.target.value))
              }
            >
              {baudRates.map((rate) => (
                <option key={rate} value={rate}>
                  {rate}
                </option>
              ))}
            </select>
          </div>

          <div className="flex gap-2 items-center">
            <div className="mb-2.5 flex-1">
              <label className="block text-xs text-text-secondary mb-1">{t(lang, 'dataBits')}</label>
              <select
                className="w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded text-sm focus:outline-none focus:border-accent transition-colors appearance-none bg-no-repeat bg-right-6 bg-center pr-6 cursor-pointer"
                value={params.data_bits}
                onChange={(e) =>
                  updateParam('data_bits', parseInt(e.target.value))
                }
              >
                <option value={5}>5</option>
                <option value={6}>6</option>
                <option value={7}>7</option>
                <option value={8}>8</option>
              </select>
            </div>
            <div className="mb-2.5 flex-1">
              <label className="block text-xs text-text-secondary mb-1">{t(lang, 'parity')}</label>
              <select
                className="w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded text-sm focus:outline-none focus:border-accent transition-colors appearance-none bg-no-repeat bg-right-6 bg-center pr-6 cursor-pointer"
                value={params.parity}
                onChange={(e) => updateParam('parity', e.target.value as SerialConfig['parity'])}
              >
                <option value="none">None</option>
                <option value="odd">Odd</option>
                <option value="even">Even</option>
              </select>
            </div>
          </div>

          <div className="flex gap-2 items-center">
            <div className="mb-2.5 flex-1">
              <label className="block text-xs text-text-secondary mb-1">{t(lang, 'stopBits')}</label>
              <select
                className="w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded text-sm focus:outline-none focus:border-accent transition-colors appearance-none bg-no-repeat bg-right-6 bg-center pr-6 cursor-pointer"
                value={params.stop_bits}
                onChange={(e) => updateParam('stop_bits', e.target.value as SerialConfig['stop_bits'])}
              >
                <option value="one">1</option>
                <option value="two">2</option>
              </select>
            </div>
            <div className="mb-2.5 flex-1">
              <label className="block text-xs text-text-secondary mb-1">{t(lang, 'flowControl')}</label>
              <select
                className="w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded text-sm focus:outline-none focus:border-accent transition-colors appearance-none bg-no-repeat bg-right-6 bg-center pr-6 cursor-pointer"
                value={params.flow_control}
                onChange={(e) => updateParam('flow_control', e.target.value as SerialConfig['flow_control'])}
              >
                <option value="none">None</option>
                <option value="software">Software</option>
                <option value="hardware">Hardware</option>
              </select>
            </div>
          </div>
        </>
      )}

      <div className="mb-2.5">
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
