import { memo, useEffect } from 'react';
import { Handle, Position, useUpdateNodeInternals, type NodeProps } from '@xyflow/react';
import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { Binary, X } from 'lucide-react';
import type { ProtocolNodeData } from '../../store/appStoreHelpers';
import { BYTES_DOMAIN_COLOR } from './TransportNode';

/// 时域端口颜色 (与 WidgetNode.domainColor 一致)
const TIME_DOMAIN_COLOR = '#75beff';

const protocolLabelKey: Record<string, string> = {
  JustFloat: 'justfloat',
  FireWater: 'firewater',
  RawData: 'rawdata',
  Slcan: 'slcan',
  CandleLight: 'candleLight',
  LogicDecode: 'logicAnalyzer',
};

/// 协议引擎 (Protocol) 全局节点 — 字节平面 + 数值帧源
/// 输入端口 in (字节), 输出端口 out (字节) + ch0..chN (数值, 各 tab 数值图的帧源)
export const ProtocolNode = memo(function ProtocolNode({ id, data }: NodeProps) {
  const lang = useAppStore((s) => s.lang);
  const removeGlobalNode = useAppStore((s) => s.removeGlobalNode);
  const rfEdges = useAppStore((s) => s.rfEdges);
  const nodeData = data as unknown as ProtocolNodeData;
  const config = nodeData.config;
  const channels = Math.max(1, nodeData.channels ?? 4);
  const updateNodeInternals = useUpdateNodeInternals();

  // 通道数变化 → 通知 React Flow 重测 handle 位置
  useEffect(() => {
    updateNodeInternals(id);
  }, [updateNodeInternals, id, channels]);

  const connectedHandles = new Set<string>();
  for (const e of rfEdges) {
    if (e.source === id && e.sourceHandle) connectedHandles.add(e.sourceHandle);
    if (e.target === id && e.targetHandle) connectedHandles.add(e.targetHandle);
  }

  const handleClass = (portId: string) =>
    `w-[9px] h-[9px] bg-bg-input border-[1.5px] rounded-full cursor-crosshair transition-all duration-150 hover:bg-accent hover:scale-130 [&.connectingto]:bg-green [&.connectingto]:border-green [&.valid]:bg-green [&.valid]:border-green${connectedHandles.has(portId) ? ' connected' : ''}`;

  return (
    <div className="nowheel widget-card-acrylic rounded-md min-w-[140px] max-w-[220px] text-[11px] relative [&.selected]:border-accent">
      <div className="flex items-center justify-between px-1.5 py-1 border-b border-border text-[10px] font-semibold uppercase tracking-[0.4px] text-accent">
        <span className="flex items-center gap-1 flex-1 truncate">
          <Binary size={11} />
          {t(lang, protocolLabelKey[config.kind] ?? 'protocolEngine')}
        </span>
        <button
          className="w-4 h-4 p-0 opacity-60 hover:opacity-100 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover transition-opacity"
          onClick={(e) => {
            e.stopPropagation();
            removeGlobalNode(id);
          }}
        >
          <X size={10} />
        </button>
      </div>
      {/* in 字节输入口 (左) */}
      <div className="absolute top-1/2 left-0 -translate-y-1/2 flex flex-col gap-0.5 py-1">
        <div className="flex items-center gap-1 h-[14px] relative pl-0.5" title={`in · ${t(lang, 'domainBytes')}`}>
          <Handle
            type="target"
            position={Position.Left}
            id="in"
            style={{ position: 'relative', left: 'auto', top: 'auto', transform: 'none', borderColor: BYTES_DOMAIN_COLOR }}
            className={handleClass('in')}
          />
          <span className="text-[9px] text-text-secondary font-mono whitespace-nowrap bg-bg-sidebar px-0.5 py-px rounded-sm">in</span>
          <span className="w-[5px] h-[5px] rounded-full flex-shrink-0 pointer-events-none" style={{ backgroundColor: BYTES_DOMAIN_COLOR }} />
        </div>
      </div>
      {/* out 字节口 + ch0..chN 数值口 (右) */}
      <div className="absolute top-1/2 right-0 -translate-y-1/2 flex flex-col items-end gap-0.5 py-1 z-10">
        <div className="flex items-center gap-1 h-[14px] relative pr-0.5" title={`out · ${t(lang, 'domainBytes')}`}>
          <span className="w-[5px] h-[5px] rounded-full flex-shrink-0 pointer-events-none" style={{ backgroundColor: BYTES_DOMAIN_COLOR }} />
          <span className="text-[9px] text-text-secondary font-mono whitespace-nowrap bg-bg-sidebar px-0.5 py-px rounded-sm">out</span>
          <Handle
            type="source"
            position={Position.Right}
            id="out"
            style={{ position: 'relative', right: 'auto', top: 'auto', transform: 'none', borderColor: BYTES_DOMAIN_COLOR }}
            className={handleClass('out')}
          />
        </div>
        {Array.from({ length: channels }, (_, i) => (
          <div key={`ch${i}`} className="flex items-center gap-1 h-[14px] relative pr-0.5" title={`ch${i} · ${t(lang, 'domainTime')}`}>
            <span className="w-[5px] h-[5px] rounded-full flex-shrink-0 pointer-events-none" style={{ backgroundColor: TIME_DOMAIN_COLOR }} />
            <span className="text-[9px] text-text-secondary font-mono whitespace-nowrap bg-bg-sidebar px-0.5 py-px rounded-sm">ch{i}</span>
            <Handle
              type="source"
              position={Position.Right}
              id={`ch${i}`}
              style={{ position: 'relative', right: 'auto', top: 'auto', transform: 'none', borderColor: TIME_DOMAIN_COLOR }}
              className={handleClass(`ch${i}`)}
            />
          </div>
        ))}
      </div>
      {/* 内容占位: 通道数摘要 */}
      <div className="px-2 py-1.5 text-[10px] font-mono text-text-secondary">
        {channels} {t(lang, 'channels')}
        {nodeData.convertTo ? ` → ${t(lang, protocolLabelKey[nodeData.convertTo.kind] ?? nodeData.convertTo.kind)}` : ''}
      </div>
    </div>
  );
});
