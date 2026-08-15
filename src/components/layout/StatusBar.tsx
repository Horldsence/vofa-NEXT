import { memo, useEffect, useRef, useState } from 'react';
import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { useContextMenu } from '../../lib/hooks/useContextMenu';
import { RefreshCw, Settings, Info } from 'lucide-react';
import clsx from 'clsx';
import { BufferUsageStats } from './BufferUsageStats';
import { CanLoadAlarm } from './CanLoadAlarm';
import { PipelineDropAlarm } from './PipelineDropAlarm';
import { useSettingsStore } from '../../store/settingsStore';

/// 底部状态栏 — 显示连接状态、统计数据
///
/// 空间不足时分级收缩 (tier, 由根节点 ResizeObserver 驱动):
/// - tier 0 (>= 960px): 全量
/// - tier 1 (< 960px): 隐藏 rx/tx frames
/// - tier 2 (< 780px): 再隐藏 transport/protocol 文本标签
/// - tier 3 (< 620px): rx/tx bytes 紧凑格式 (↓ 1.2MB / ↑ 0B), 隐藏 BufferUsageStats
/// 任何 tier 保留: 连接状态、两个告警、刷新按钮
// 断点阈值 (px) — 按当前内容实测宽度估计, 可按需调整
const TIER1_MAX = 960;
const TIER2_MAX = 780;
const TIER3_MAX = 620;

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

  // 本地 tier 状态 — 只影响渲染, 不改变上方标量订阅纪律
  const rootRef = useRef<HTMLDivElement>(null);
  const [barWidth, setBarWidth] = useState(Number.POSITIVE_INFINITY);
  useEffect(() => {
    const el = rootRef.current;
    if (!el) return;
    const ro = new ResizeObserver((entries) => {
      setBarWidth(entries[0].contentRect.width);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  const tier =
    barWidth >= TIER1_MAX ? 0 : barWidth >= TIER2_MAX ? 1 : barWidth >= TIER3_MAX ? 2 : 3;

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
    <div ref={rootRef} className="h-[24px] bg-bg-statusbar text-text-secondary flex items-center px-2 text-xs gap-3 flex-shrink-0 overflow-hidden" onContextMenu={onContextMenu}>
      <div className="flex items-center gap-1.5 h-full">
        <span className={clsx("w-2.5 h-2.5 rounded-full inline-block flex-shrink-0", dotColorClass)} />
        <span className="whitespace-nowrap">{stateLabel[connectionState]}</span>
      </div>
      {tier < 2 && (
        <>
          <div className="flex items-center gap-1.5 h-full whitespace-nowrap">
            {transportLabel[transportKind]}
          </div>
          <div className="flex items-center gap-1.5 h-full whitespace-nowrap">
            {protocolLabel[protocolKind]}
          </div>
        </>
      )}
      <div className="flex-1" />
      {tier >= 3 ? (
        <>
          <div
            className="flex items-center gap-1 h-full whitespace-nowrap tabular-nums"
            title={`${t(lang, 'rxBytes')}: ${formatBytes(rxBytes)}`}
          >
            ↓ {formatBytes(rxBytes)}
          </div>
          <div
            className="flex items-center gap-1 h-full whitespace-nowrap tabular-nums"
            title={`${t(lang, 'txBytes')}: ${formatBytes(txBytes)}`}
          >
            ↑ {formatBytes(txBytes)}
          </div>
        </>
      ) : (
        <>
          <div className="flex items-center gap-1.5 h-full whitespace-nowrap tabular-nums">
            {t(lang, 'rxBytes')}: {formatBytes(rxBytes)}
          </div>
          <div className="flex items-center gap-1.5 h-full whitespace-nowrap tabular-nums">
            {t(lang, 'txBytes')}: {formatBytes(txBytes)}
          </div>
        </>
      )}
      {tier < 1 && (
        <>
          <div className="flex items-center gap-1.5 h-full whitespace-nowrap tabular-nums">
            {t(lang, 'rxFrames')}: {rxFrames}
          </div>
          <div className="flex items-center gap-1.5 h-full whitespace-nowrap tabular-nums">
            {t(lang, 'txFrames')}: {txFrames}
          </div>
        </>
      )}
      <div className="w-px h-3 bg-border-subtle mx-1 flex-shrink-0" />
      <CanLoadAlarm />
      <PipelineDropAlarm />
      {tier < 3 && <BufferUsageStats />}
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
