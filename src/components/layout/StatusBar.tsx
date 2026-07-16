import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { RefreshCw } from 'lucide-react';

/// 底部状态栏 — 显示连接状态、统计数据
export function StatusBar() {
  const lang = useAppStore((s) => s.lang);
  const connectionState = useAppStore((s) => s.connectionState);
  const stats = useAppStore((s) => s.stats);
  const transportConfig = useAppStore((s) => s.transportConfig);
  const protocolConfig = useAppStore((s) => s.protocolConfig);
  const refreshPorts = useAppStore((s) => s.refreshPorts);

  const stateLabel: Record<typeof connectionState, string> = {
    Disconnected: t(lang, 'disconnected'),
    Connecting: t(lang, 'connecting'),
    Connected: t(lang, 'connected'),
    Error: 'Error',
  };

  const transportLabel: Record<string, string> = {
    Serial: t(lang, 'serial'),
    Udp: t(lang, 'udp'),
    TcpClient: t(lang, 'tcpClient'),
    TcpServer: t(lang, 'tcpServer'),
    TestData: t(lang, 'testData'),
  };

  const protocolLabel: Record<string, string> = {
    JustFloat: t(lang, 'justfloat'),
    FireWater: t(lang, 'firewater'),
    RawData: t(lang, 'rawdata'),
  };

  const formatBytes = (n: number) => {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    return `${(n / 1024 / 1024).toFixed(2)} MB`;
  };

  return (
    <div className="status-bar">
      <div className="status-item">
        <span className={`state-dot ${connectionState.toLowerCase()}`} />
        <span>{stateLabel[connectionState]}</span>
      </div>
      <div className="status-item">
        {transportLabel[transportConfig.kind]}
      </div>
      <div className="status-item">
        {protocolLabel[protocolConfig.kind]}
      </div>
      <div className="status-spacer" />
      <div className="status-item">
        {t(lang, 'rxBytes')}: {formatBytes(stats.rx_bytes)}
      </div>
      <div className="status-item">
        {t(lang, 'txBytes')}: {formatBytes(stats.tx_bytes)}
      </div>
      <div className="status-item">
        {t(lang, 'rxFrames')}: {stats.rx_frames}
      </div>
      <button
        className="btn-icon"
        style={{ color: 'var(--text-bright)' }}
        title={t(lang, 'refresh')}
        onClick={() => refreshPorts()}
      >
        <RefreshCw size={12} />
      </button>
    </div>
  );
}
