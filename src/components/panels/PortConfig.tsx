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
    <div>
      <div className="form-group">
        <div className="form-row" style={{ marginBottom: 6 }}>
          <span className="form-label" style={{ margin: 0 }}>
            {t(lang, 'portName')}
          </span>
          <button
            className="btn-icon"
            title={t(lang, 'refresh')}
            onClick={() => refreshPorts()}
          >
            <RefreshCw size={14} />
          </button>
        </div>
        <div className="port-filter">
          <Search size={12} />
          <input
            type="text"
            placeholder={lang === 'zh' ? '筛选端口...' : 'Filter ports...'}
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
        </div>
        <div className="port-list">
          {ports.length === 0 ? (
            <div
              style={{
                padding: 12,
                color: 'var(--text-secondary)',
                fontSize: 11,
                textAlign: 'center',
              }}
            >
              {lang === 'zh' ? '未发现串口' : 'No ports found'}
            </div>
          ) : filteredPorts.length === 0 ? (
            <div
              style={{
                padding: 12,
                color: 'var(--text-secondary)',
                fontSize: 11,
                textAlign: 'center',
              }}
            >
              {lang === 'zh' ? '无匹配端口' : 'No matching ports'}
            </div>
          ) : (
            filteredPorts.map(({ port, idx }) => (
              <div
                key={port.name}
                className={`port-item ${idx === selectedPortIndex ? 'selected' : ''}`}
                onClick={() => selectPort(idx)}
              >
                <div className="port-name">
                  <Usb size={10} style={{ display: 'inline', marginRight: 4 }} />
                  {port.name}
                </div>
                {(port.product || port.manufacturer) && (
                  <div className="port-info">
                    <Cpu size={9} style={{ display: 'inline', marginRight: 3, verticalAlign: 'middle' }} />
                    {[port.product, port.manufacturer]
                      .filter(Boolean)
                      .join(' • ')}
                  </div>
                )}
                {(port.vid !== null || port.pid !== null) && (
                  <div className="port-info port-info-vidpid">
                    VID:{port.vid !== null ? port.vid.toString(16).toUpperCase().padStart(4, '0') : '----'}
                    {'  PID:'}
                    {port.pid !== null ? port.pid.toString(16).toUpperCase().padStart(4, '0') : '----'}
                    {'  ('}
                    {port.port_type}
                    {')'}
                  </div>
                )}
                {port.serial_number && (
                  <div className="port-info port-info-serial">
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
          <div className="form-group">
            <label className="form-label">{t(lang, 'baudRate')}</label>
            <select
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

          <div className="form-row">
            <div className="form-group">
              <label className="form-label">{t(lang, 'dataBits')}</label>
              <select
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
            <div className="form-group">
              <label className="form-label">{t(lang, 'parity')}</label>
              <select
                value={params.parity}
                onChange={(e) => updateParam('parity', e.target.value as SerialConfig['parity'])}
              >
                <option value="none">None</option>
                <option value="odd">Odd</option>
                <option value="even">Even</option>
              </select>
            </div>
          </div>

          <div className="form-row">
            <div className="form-group">
              <label className="form-label">{t(lang, 'stopBits')}</label>
              <select
                value={params.stop_bits}
                onChange={(e) => updateParam('stop_bits', e.target.value as SerialConfig['stop_bits'])}
              >
                <option value="one">1</option>
                <option value="two">2</option>
              </select>
            </div>
            <div className="form-group">
              <label className="form-label">{t(lang, 'flowControl')}</label>
              <select
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

      <div className="form-group">
        {isConnected ? (
          <button
            className="btn btn-danger w-full"
            onClick={() => disconnect()}
          >
            <PlugZap size={14} />
            {t(lang, 'disconnect')}
          </button>
        ) : (
          <button
            className="btn w-full"
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
