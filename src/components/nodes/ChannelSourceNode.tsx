import { Handle, Position, type NodeProps } from '@xyflow/react';
import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { Radio as RadioIcon } from 'lucide-react';

/// 通道源节点 — 表示协议数据来源, 输出 ch0, ch1, ... 通道
/// 不可删除, 自动创建于每个 tab
export function ChannelSourceNode({ id, data }: NodeProps) {
  const lang = useAppStore((s) => s.lang);
  const channelCount = (data.channelCount as number) ?? 4;
  const detectedChannels = useAppStore((s) => s.detectedChannels);
  const protocolConfig = useAppStore((s) => s.protocolConfig);
  const rfEdges = useAppStore((s) => s.rfEdges);
  // 已连接的通道端口 — 用于 Handle 实色填充
  const connectedHandles = new Set<string>();
  for (const e of rfEdges) {
    if (e.source === id && e.sourceHandle) connectedHandles.add(e.sourceHandle);
  }

  const isAuto = (protocolConfig.kind === 'JustFloat' || protocolConfig.kind === 'FireWater') && protocolConfig.channels == null;
  const label = isAuto
    ? (detectedChannels != null
      ? `${t(lang, 'channelSource')} (${detectedChannels})`
      : `${t(lang, 'channelSource')} (${t(lang, 'channelsAuto')})`)
    : `${t(lang, 'channelSource')} (${channelCount})`;

  return (
    <div
      className="border border-border rounded-md min-w-[140px] text-[11px] [&.selected]:border-accent"
      style={{ backgroundColor: 'color-mix(in srgb, var(--color-accent) 25%, var(--color-bg-sidebar))' }}
    >
      <div
        className="flex items-center gap-1.5 px-2 py-1 border-b border-border text-[10px] font-semibold uppercase tracking-[0.4px]"
        style={{ color: 'var(--accent)' }}
      >
        <RadioIcon size={12} />
        <span>{label}</span>
      </div>
      <div className="flex flex-col gap-1 px-2.5 py-1.5">
        {Array.from({ length: channelCount }, (_, i) => (
          <div key={i} className="flex items-center justify-between gap-2 relative">
            <span className="font-mono text-[10px] text-text-primary">ch{i}</span>
            <Handle
              type="source"
              position={Position.Right}
              id={`ch${i}`}
              className={`w-[9px] h-[9px] bg-bg-input border-[1.5px] border-accent rounded-full cursor-crosshair transition-all duration-150 hover:bg-accent hover:scale-130 [&.connectingto]:bg-green [&.connectingto]:border-green [&.valid]:bg-green [&.valid]:border-green${connectedHandles.has(`ch${i}`) ? ' connected' : ''}`}
            />
          </div>
        ))}
      </div>
    </div>
  );
}
