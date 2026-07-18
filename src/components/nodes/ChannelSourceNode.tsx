import { Handle, Position, type NodeProps } from '@xyflow/react';
import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { Radio as RadioIcon } from 'lucide-react';

/// 通道源节点 — 表示协议数据来源, 输出 ch0, ch1, ... 通道
/// 不可删除, 自动创建于每个 tab
export function ChannelSourceNode({ data }: NodeProps) {
  const lang = useAppStore((s) => s.lang);
  const channelCount = (data.channelCount as number) ?? 4;
  const detectedChannels = useAppStore((s) => s.detectedChannels);
  const protocolConfig = useAppStore((s) => s.protocolConfig);

  const isAuto = protocolConfig.kind !== 'RawData' && protocolConfig.channels == null;
  const label = isAuto
    ? (detectedChannels != null
      ? `${t(lang, 'channelSource')} (${detectedChannels})`
      : `${t(lang, 'channelSource')} (${t(lang, 'channelsAuto')})`)
    : `${t(lang, 'channelSource')} (${channelCount})`;

  return (
    <div className="bg-gradient-to-br from-[#1a3a5c] to-[#2a5a8c] border border-accent rounded-md min-w-[140px] p-0 shadow-[0_2px_8px_rgba(0,0,0,0.4)] text-[11px]">
      <div className="flex items-center gap-1.5 px-2.5 py-1.5 bg-[rgba(255,255,255,0.06)] border-b border-[rgba(255,255,255,0.1)] rounded-t-md text-[10px] font-semibold text-text-bright uppercase tracking-[0.4px]">
        <RadioIcon size={12} />
        <span>{label}</span>
      </div>
      <div className="flex flex-col gap-1 px-2.5 py-1.5">
        {Array.from({ length: channelCount }, (_, i) => (
          <div key={i} className="flex items-center justify-between gap-2 relative">
            <span className="font-mono text-[10px] text-text-bright">ch{i}</span>
            <Handle
              type="source"
              position={Position.Right}
              id={`ch${i}`}
              className="w-[9px] h-[9px] bg-bg-input border-[1.5px] border-accent rounded-full cursor-crosshair transition-all duration-150 hover:bg-accent hover:scale-130 [&.connectingto]:bg-green [&.connectingto]:border-green [&.valid]:bg-green [&.valid]:border-green"
            />
          </div>
        ))}
      </div>
    </div>
  );
}
