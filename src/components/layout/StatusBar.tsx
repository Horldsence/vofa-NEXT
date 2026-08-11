import { memo } from 'react';
import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { useContextMenu } from '../../lib/hooks/useContextMenu';
import { RefreshCw, Settings, Info } from 'lucide-react';
import clsx from 'clsx';
import { BufferUsageStats } from './BufferUsageStats';
import { CanLoadAlarm } from './CanLoadAlarm';
import { useSettingsStore } from '../../store/settingsStore';

/// 底部状态栏 — 显示连接状态、统计数据
export const StatusBar = memo(function StatusBar() {
  const lang = useAppStore((s) => s.lang);
  const connectionState = useAppStore((s) => s.connectionState);
  // 单独订阅 stats 字段, 避免 transport:rx 每次创建新 stats 对象导致整个 StatusBar 重渲染
  const rxBytes = useAppStore((s) => s.stats.rx_bytes);
  const txBytes = useAppStore((s) => s.stats.tx_bytes);
  const rxFrames = useAppStore((s) => s.stats.rx_frames);
  const txFrames = useAppStore((s) => s.stats.tx_frames);
  // 仅订阅 kind 标量 — 修改传输/协议参数 (如串口端口名) 不触发状态栏重渲染
  const transportKind = useAppStore((s) => s.transportConfig.kind);
  const protocolKind = useAppStore((s) => s.protocolConfig.kind);
  const refreshPorts = useAppStore((s) => s.refreshPorts);
  const openSettings = useSettingsStore((s) => s.open);
  const openAbout = useSettingsStore((s) => s.openAbout);

  const onContextMenu = useContextMenu([
    {
      id: 'refresh-ports',
      label: t(lang, 'refresh'),
      icon: <RefreshCw />,
      onClick: () => refreshPorts(),
    },
    { kind: 'separator' },
    {
      id: 'settings',
      label: t(lang, 'settings'),
      icon: <Settings />,
      onClick: openSettings,
    },
    {
      id: 'about',
      label: t(lang, 'about'),
      icon: <Info />,
      onClick: openAbout,
    },
  ]);

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
    Disconnected: 'bg-text-muted',
    Connecting: 'bg-yellow animate-pulse',
    Connected: 'bg-green',
    Error: 'bg-red',
  }[connectionState];

  return (
    <div className="h-[24px] bg-bg-statusbar text-text-secondary flex items-center px-2 text-xs gap-3 flex-shrink-0" onContextMenu={onContextMenu}>
      <div className="flex items-center gap-1.5 h-full">
        <span className={clsx("w-2.5 h-2.5 rounded-full inline-block flex-shrink-0", dotColorClass)} />
        <span className="whitespace-nowrap">{stateLabel[connectionState]}</span>
      </div>
      <div className="flex items-center gap-1.5 h-full whitespace-nowrap">
        {transportLabel[transportKind]}
      </div>
      <div className="flex items-center gap-1.5 h-full whitespace-nowrap">
        {protocolLabel[protocolKind]}
      </div>
      <div className="flex-1" />
      <div className="flex items-center gap-1.5 h-full whitespace-nowrap tabular-nums">
        {t(lang, 'rxBytes')}: {formatBytes(rxBytes)}
      </div>
      <div className="flex items-center gap-1.5 h-full whitespace-nowrap tabular-nums">
        {t(lang, 'txBytes')}: {formatBytes(txBytes)}
      </div>
      <div className="flex items-center gap-1.5 h-full whitespace-nowrap tabular-nums">
        {t(lang, 'rxFrames')}: {rxFrames}
      </div>
      <div className="flex items-center gap-1.5 h-full whitespace-nowrap tabular-nums">
        {t(lang, 'txFrames')}: {txFrames}
      </div>
      <div className="w-px h-3 bg-border-subtle mx-1 flex-shrink-0" />
      <CanLoadAlarm />
      <BufferUsageStats />
      <div className="w-px h-3 bg-border-subtle mx-1 flex-shrink-0" />
      <button
        className="w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary active:bg-accent-active transition-colors duration-150 flex-shrink-0"
        title={t(lang, 'refresh')}
        onClick={() => refreshPorts()}
      >
        <RefreshCw size={12} />
      </button>
    </div>
  );
});
