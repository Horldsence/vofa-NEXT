import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { RefreshCw } from 'lucide-react';
import clsx from 'clsx';

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

  const dotColorClass = {
    Disconnected: 'bg-text-secondary',
    Connecting: 'bg-yellow animate-pulse',
    Connected: 'bg-green',
    Error: 'bg-red',
  }[connectionState];

  return (
    <div className="h-[22px] bg-bg-statusbar text-text-bright flex items-center px-2 text-xs gap-3 flex-shrink-0">
      <div className="flex items-center gap-1">
        <span className={clsx("w-2 h-2 rounded-full inline-block", dotColorClass)} />
        <span>{stateLabel[connectionState]}</span>
      </div>
      <div className="flex items-center gap-1">
        {transportLabel[transportConfig.kind]}
      </div>
      <div className="flex items-center gap-1">
        {protocolLabel[protocolConfig.kind]}
      </div>
      <div className="flex-1" />
      <div className="flex items-center gap-1">
        {t(lang, 'rxBytes')}: {formatBytes(stats.rx_bytes)}
      </div>
      <div className="flex items-center gap-1">
        {t(lang, 'txBytes')}: {formatBytes(stats.tx_bytes)}
      </div>
      <div className="flex items-center gap-1">
        {t(lang, 'rxFrames')}: {stats.rx_frames}
      </div>
      <button
        className="w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors duration-150"
        style={{ color: 'var(--text-bright)' }}
        title={t(lang, 'refresh')}
        onClick={() => refreshPorts()}
      >
        <RefreshCw size={12} />
      </button>
    </div>
  );
}
