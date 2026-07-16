import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { ArrowRight, Info } from 'lucide-react';
import type {
  TransportConfig,
  UdpConfig,
  TcpClientConfig,
  TcpServerConfig,
  TestDataConfig,
} from '../../types';

/// 数据接口配置面板 — Serial/UDP/TCP Client/TCP Server/TestData
/// 注意: 串口端口选择已在 PortConfig 面板完成, 此处不再重复
export function TransportConfigPanel() {
  const lang = useAppStore((s) => s.lang);
  const transportConfig = useAppStore((s) => s.transportConfig);
  const setTransportConfig = useAppStore((s) => s.setTransportConfig);
  const setSidebarView = useAppStore((s) => s.setSidebarView);

  const kinds: { value: TransportConfig['kind']; label: string }[] = [
    { value: 'Serial', label: t(lang, 'serial') },
    { value: 'Udp', label: t(lang, 'udp') },
    { value: 'TcpClient', label: t(lang, 'tcpClient') },
    { value: 'TcpServer', label: t(lang, 'tcpServer') },
    { value: 'TestData', label: t(lang, 'testData') },
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
          params: { channels: 4, sample_rate: 100, signal: 'sine' },
        };
        break;
    }
    setTransportConfig(defaults);
  };

  const update = <K extends TransportConfig['kind']>(
    patch: Partial<
      K extends 'Udp' ? UdpConfig :
      K extends 'TcpClient' ? TcpClientConfig :
      K extends 'TcpServer' ? TcpServerConfig :
      TestDataConfig
    >
  ) => {
    if (transportConfig.kind === 'Udp') {
      setTransportConfig({
        kind: 'Udp',
        params: { ...transportConfig.params, ...patch },
      });
    } else if (transportConfig.kind === 'TcpClient') {
      setTransportConfig({
        kind: 'TcpClient',
        params: { ...transportConfig.params, ...patch },
      });
    } else if (transportConfig.kind === 'TcpServer') {
      setTransportConfig({
        kind: 'TcpServer',
        params: { ...transportConfig.params, ...patch },
      });
    } else if (transportConfig.kind === 'TestData') {
      setTransportConfig({
        kind: 'TestData',
        params: { ...transportConfig.params, ...patch },
      });
    }
  };

  const signalOptions: { value: TestDataConfig['signal']; label: string }[] = [
    { value: 'sine', label: t(lang, 'sine') },
    { value: 'square', label: t(lang, 'square') },
    { value: 'triangle', label: t(lang, 'triangle') },
    { value: 'sawtooth', label: t(lang, 'sawtooth') },
    { value: 'random', label: t(lang, 'random') },
    { value: 'dc', label: t(lang, 'dc') },
    { value: 'chirp', label: t(lang, 'chirp') },
    { value: 'steps', label: t(lang, 'steps') },
    { value: 'noise', label: t(lang, 'noise') },
    { value: 'multitone', label: t(lang, 'multitone') },
  ];

  return (
    <div>
      <div className="form-group">
        <label className="form-label">{t(lang, 'transportType')}</label>
        <select
          value={transportConfig.kind}
          onChange={(e) => switchKind(e.target.value as TransportConfig['kind'])}
        >
          {kinds.map((k) => (
            <option key={k.value} value={k.value}>
              {k.label}
            </option>
          ))}
        </select>
      </div>

      {/* 串口模式: 端口和参数在 PortConfig 面板配置, 此处仅提示 */}
      {transportConfig.kind === 'Serial' && (
        <div
          style={{
            padding: 12,
            background: 'var(--bg-input)',
            borderRadius: 4,
            fontSize: 11,
            color: 'var(--text-secondary)',
            lineHeight: 1.6,
            display: 'flex',
            gap: 8,
          }}
        >
          <Info size={14} style={{ flexShrink: 0, marginTop: 1 }} />
          <span>
            {lang === 'zh'
              ? '串口端口选择和参数配置 (波特率/数据位/校验/停止位/流控) 请在"串口配置"面板中完成。'
              : 'Port selection and serial parameters (baud/data bits/parity/stop/flow) are configured in the "Port Config" panel.'}
          </span>
        </div>
      )}

      {transportConfig.kind === 'Udp' && (
        <>
          <div className="form-group">
            <label className="form-label">{t(lang, 'localAddr')}</label>
            <input
              type="text"
              value={transportConfig.params.local_addr}
              onChange={(e) => update<'Udp'>({ local_addr: e.target.value })}
            />
          </div>
          <div className="form-group">
            <label className="form-label">{t(lang, 'localPort')}</label>
            <input
              type="number"
              value={transportConfig.params.local_port}
              onChange={(e) =>
                update<'Udp'>({ local_port: parseInt(e.target.value) || 0 })
              }
            />
          </div>
          <div className="form-group">
            <label className="form-label">{t(lang, 'remoteAddr')}</label>
            <input
              type="text"
              value={transportConfig.params.remote_addr}
              onChange={(e) => update<'Udp'>({ remote_addr: e.target.value })}
            />
          </div>
          <div className="form-group">
            <label className="form-label">{t(lang, 'remotePort')}</label>
            <input
              type="number"
              value={transportConfig.params.remote_port}
              onChange={(e) =>
                update<'Udp'>({ remote_port: parseInt(e.target.value) || 0 })
              }
            />
          </div>
        </>
      )}

      {transportConfig.kind === 'TcpClient' && (
        <>
          <div className="form-group">
            <label className="form-label">{t(lang, 'host')}</label>
            <input
              type="text"
              value={transportConfig.params.host}
              onChange={(e) => update<'TcpClient'>({ host: e.target.value })}
            />
          </div>
          <div className="form-group">
            <label className="form-label">{t(lang, 'port')}</label>
            <input
              type="number"
              value={transportConfig.params.port}
              onChange={(e) =>
                update<'TcpClient'>({ port: parseInt(e.target.value) || 0 })
              }
            />
          </div>
        </>
      )}

      {transportConfig.kind === 'TcpServer' && (
        <>
          <div className="form-group">
            <label className="form-label">{t(lang, 'listenAddr')}</label>
            <input
              type="text"
              value={transportConfig.params.listen_addr}
              onChange={(e) =>
                update<'TcpServer'>({ listen_addr: e.target.value })
              }
            />
          </div>
          <div className="form-group">
            <label className="form-label">{t(lang, 'listenPort')}</label>
            <input
              type="number"
              value={transportConfig.params.listen_port}
              onChange={(e) =>
                update<'TcpServer'>({ listen_port: parseInt(e.target.value) || 0 })
              }
            />
          </div>
        </>
      )}

      {transportConfig.kind === 'TestData' && (
        <>
          <div className="form-group">
            <label className="form-label">{t(lang, 'channels')}</label>
            <input
              type="number"
              min={1}
              max={32}
              value={transportConfig.params.channels}
              onChange={(e) =>
                update<'TestData'>({ channels: parseInt(e.target.value) || 1 })
              }
            />
          </div>
          <div className="form-group">
            <label className="form-label">{t(lang, 'sampleRate')}</label>
            <input
              type="number"
              min={1}
              max={10000}
              value={transportConfig.params.sample_rate}
              onChange={(e) =>
                update<'TestData'>({ sample_rate: parseInt(e.target.value) || 1 })
              }
            />
          </div>
          <div className="form-group">
            <label className="form-label">{t(lang, 'signalType')}</label>
            <select
              value={transportConfig.params.signal}
              onChange={(e) =>
                update<'TestData'>({
                  signal: e.target.value as TestDataConfig['signal'],
                })
              }
            >
              {signalOptions.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {opt.label}
                </option>
              ))}
            </select>
          </div>
        </>
      )}

      {/* 跳转到协议引擎配置 */}
      <div className="form-group" style={{ marginTop: 16 }}>
        <button
          className="btn w-full"
          onClick={() => setSidebarView('protocol')}
        >
          {t(lang, 'nextProtocol')}
          <ArrowRight size={14} />
        </button>
      </div>
    </div>
  );
}
