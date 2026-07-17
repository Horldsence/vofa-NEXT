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
    <div className="rf-channel-source">
      <div className="rf-channel-source-header">
        <RadioIcon size={12} />
        <span>{label}</span>
      </div>
      <div className="rf-channel-source-channels">
        {Array.from({ length: channelCount }, (_, i) => (
          <div key={i} className="rf-channel-source-row">
            <span className="rf-channel-source-label">ch{i}</span>
            <Handle
              type="source"
              position={Position.Right}
              id={`ch${i}`}
              className="rf-handle rf-handle-source"
            />
          </div>
        ))}
      </div>
    </div>
  );
}
