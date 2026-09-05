import { memo, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { useContextMenu } from '../../lib/hooks/useContextMenu';
import { RefreshCw, Settings, Info, Sparkles } from 'lucide-react';
import clsx from 'clsx';
import { CanLoadAlarm } from './CanLoadAlarm';
import { PipelineDropAlarm } from './PipelineDropAlarm';
import { UpdateIndicator } from './UpdateIndicator';
import { useSettingsStore } from '../../store/settingsStore';
import { usePrimaryProtocolConfig, usePrimaryTransportConfig } from '../../lib/hooks/usePrimaryNodes';
import CompileStatusIndicator from './CompileStatusIndicator';
import { RunControls } from './RunControls';
import { useLayoutStore } from '../../store/layoutStore';

/// AI 对话面板开关 — 状态栏右侧常驻入口 (面板可见性持久化, 会话在后端不丢失)
function AiPanelToggle() {
  const lang = useAppStore((s) => s.lang);
  const visible = useLayoutStore((s) => s.aiPanelVisible);
  const setAiPanelVisible = useLayoutStore((s) => s.setAiPanelVisible);
  return (
    <button
      className={`w-6 h-6 flex items-center justify-center rounded transition-colors duration-150 shrink-0 ${
        visible
          ? 'text-accent bg-accent/10'
          : 'text-text-secondary hover:bg-bg-hover hover:text-text-primary'
      }`}
      title={t(lang, 'aiChat')}
      onClick={() => setAiPanelVisible(!visible)}
    >
      <Sparkles size={12} />
    </button>
  );
}

/// 底部状态栏 — 显示连接状态、统计数据
///
/// 空间不足时分级收缩 (tier), 由内容实际溢出驱动而非固定像素断点 —
/// 窗口最小宽度 (minWidth 900) 之上的所有宽度、不同语言标签长度、
/// 告警出现/消失都能自适应, 永远不会裁切内容:
/// - tier 0: 全量
/// - tier 1: 隐藏 rx/tx frames
/// - tier 2: 再隐藏 transport/protocol 文本标签
/// - tier 3: rx/tx bytes 紧凑格式 (↓ 1.2MB / ↑ 0B)
/// 任何 tier 保留: 连接状态、两个告警、刷新按钮
const TIER_MAX = 3;

export const StatusBar = memo(function StatusBar() {
  const lang = useAppStore((s) => s.lang);
  // 多连接并存: 状态栏显示第一个 Transport 节点的状态; 统计为全部节点合计
  const connectionState = useAppStore((s) => {
    const first = s.rfNodes.find((n) => n.type === 'transport' && n.data?.global === true);
    return (first ? s.connectionStates[first.id] : undefined) ?? 'Disconnected';
  });
  // 单独订阅合计标量, 避免 transport:rx 每次创建新对象导致整个 StatusBar 重渲染
  const rxBytes = useAppStore((s) => Object.values(s.nodeStats).reduce((a, v) => a + v.rx_bytes, 0));
  const txBytes = useAppStore((s) => Object.values(s.nodeStats).reduce((a, v) => a + v.tx_bytes, 0));
  const rxFrames = useAppStore((s) => Object.values(s.nodeStats).reduce((a, v) => a + v.rx_frames, 0));
  const txFrames = useAppStore((s) => Object.values(s.nodeStats).reduce((a, v) => a + v.tx_frames, 0));
  // 仅订阅 kind 标量 — 修改传输/协议参数 (如串口端口名) 不触发状态栏重渲染
  const transportKind = usePrimaryTransportConfig()?.kind;
  const protocolKind = usePrimaryProtocolConfig()?.kind;
  const refreshPorts = useAppStore((s) => s.refreshPorts);
  const openSettings = useSettingsStore((s) => s.open);
  const openAbout = useSettingsStore((s) => s.openAbout);

  // 本地 tier 状态 — 只影响渲染, 不改变上方标量订阅纪律
  const rootRef = useRef<HTMLDivElement>(null);
  const [tier, setTier] = useState(0);
  const lastWidth = useRef(0);

  // ResizeObserver 是唯一可靠的 tier 触发源 — 断开连接时没有数据事件,
  // 状态栏不会重渲染, 因此必须在 RO 回调里直接驱动两个方向的调整:
  // 变宽: 逐级尝试展开更多内容 (是否真放得下由下方 layout effect 校正)
  // 变窄且已溢出: 逐级收缩
  useEffect(() => {
    const el = rootRef.current;
    if (!el) return;
    // rAF 延迟 setState: 避免 RO 回调内同步改布局导致的 ResizeObserver loop 告警
    let raf: number | null = null;
    const ro = new ResizeObserver((entries) => {
      const w = entries[0].contentRect.width;
      if (raf !== null) cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => {
        raf = null;
        const grew = w > lastWidth.current;
        lastWidth.current = w;
        setTier((t) => {
          if (grew) return Math.max(0, t - 1);
          if (el.scrollWidth > el.clientWidth + 1) return Math.min(TIER_MAX, t + 1);
          return t;
        });
      });
    });
    ro.observe(el);
    return () => {
      ro.disconnect();
      if (raf !== null) cancelAnimationFrame(raf);
    };
  }, []);

  // 溢出校正: 每次渲染后(绘制前)检测实际溢出, 逐级收缩直到放下或到达 TIER_MAX.
  // 同时覆盖内容自身变宽的场景 (统计数字增长、告警出现、语言切换)
  useLayoutEffect(() => {
    const el = rootRef.current;
    if (el && el.scrollWidth > el.clientWidth + 1 && tier < TIER_MAX) {
      setTier(tier + 1);
    }
  }, [tier]);

  const onContextMenu = useContextMenu([
    {
      id: 'refresh-ports',
      label: t(lang, 'refresh'),
      icon: <RefreshCw />,
      onClick: () => { void refreshPorts(); },
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
    <div ref={rootRef} className="h-[24px] bg-bg-statusbar text-text-secondary flex items-center px-2 text-xs gap-3 shrink-0 overflow-hidden" onContextMenu={onContextMenu}>
      <RunControls />
      <div className="w-px h-3 bg-border-subtle mx-0.5 shrink-0" />
      <div className="flex items-center gap-1.5 h-full">
        <span className={clsx("w-2.5 h-2.5 rounded-full inline-block shrink-0", dotColorClass)} />
        <span className="whitespace-nowrap">{stateLabel[connectionState]}</span>
      </div>
      {tier < 2 && (
        <>
          <div className="flex items-center gap-1.5 h-full whitespace-nowrap">
            {transportKind ? transportLabel[transportKind] : '—'}
          </div>
          <div className="flex items-center gap-1.5 h-full whitespace-nowrap">
            {protocolKind ? protocolLabel[protocolKind] : '—'}
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
      <div className="flex items-center gap-1.5 h-full">
        <CompileStatusIndicator
          compact={tier >= 2}
          onClickError={() => useAppStore.getState().addCompileErrorsTab()}
        />
      </div>
      <div className="w-px h-3 bg-border-subtle mx-1 shrink-0" />
      <CanLoadAlarm />
      <PipelineDropAlarm />
      <UpdateIndicator />
      <div className="w-px h-3 bg-border-subtle mx-1 shrink-0" />
      <button
        className="w-6 h-6 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover hover:text-text-primary active:bg-accent-active transition-colors duration-150 shrink-0"
        title={t(lang, 'refresh')}
        onClick={() => { void refreshPorts(); }}
      >
        <RefreshCw size={12} />
      </button>
      <AiPanelToggle />
    </div>
  );
});
