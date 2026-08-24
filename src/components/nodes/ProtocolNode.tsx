import { memo, useEffect } from 'react';
import { Handle, Position, useUpdateNodeInternals, type NodeProps } from '@xyflow/react';
import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { Binary, X } from 'lucide-react';
import type { ProtocolNodeData } from '../../store/appStoreHelpers';
import { isRawDataPreset, protocolPortNames } from '../../lib/utils/protocolSchema';
import { BYTES_DOMAIN_COLOR } from './TransportNode';
import { CanvasErrorTooltip, useCanvasNodeError } from '../ui/CanvasErrorTooltip';

/// 时域端口颜色 (与 WidgetNode.domainColor 一致)
const TIME_DOMAIN_COLOR = '#75beff';
/// 字符串域端口颜色 (与 WidgetNode.domainColor 的 string 分支一致)
const STRING_DOMAIN_COLOR = '#ffa726';

const protocolLabelKey: Record<string, string> = {
  JustFloat: 'justfloat',
  FireWater: 'firewater',
  RawData: 'rawdata',
  Slcan: 'slcan',
  CandleLight: 'candleLight',
  LogicDecode: 'logicAnalyzer',
};

/// 协议引擎 (Protocol) 全局节点 — 字节平面 + 数值帧源
/// 输入端口 in (字节), 输出端口 out (字节) + ch0..chN (数值, 各 tab 数值图的帧源);
/// RawData 预设不产数值帧, 数值口为单个 str (字符串域)
///
/// 端口表读取顺序: derivedPorts (后端 graph:derived 单一权威) → protocolPortNames (UI 兜底,
/// 用于 derived 尚未到达或节点尚未 sync 时的瞬间)。derived 一旦到达即覆盖, 不再本地推导。
export const ProtocolNode = memo(function ProtocolNode({ id, data }: NodeProps) {
  const lang = useAppStore((s) => s.lang);
  const removeGlobalNode = useAppStore((s) => s.removeGlobalNode);
  const rfEdges = useAppStore((s) => s.rfEdges);
  const derivedPorts = useAppStore((s) => s.derivedPorts[id]);
  const detectedChannels = useAppStore((s) => s.detectedChannels[id] ?? null);
  const errorMessage = useCanvasNodeError(id, undefined);
  const nodeData = data as unknown as ProtocolNodeData;
  const config = nodeData.config;
  // 优先读后端 derived; 兜底用 protocolPortNames (节点刚创建/连接瞬间 derived 未到)
  const ports = derivedPorts?.ports.map((p) => p.name) ?? protocolPortNames(nodeData, detectedChannels);
  const rawData = isRawDataPreset(nodeData);
  const channels = ports.length;
  const portsKey = ports.join('');
  const updateNodeInternals = useUpdateNodeInternals();

  // 端口数/命名变化 → 通知 React Flow 重测 handle 位置
  useEffect(() => {
    updateNodeInternals(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [updateNodeInternals, id, portsKey]);

  const connectedHandles = new Set<string>();
  for (const e of rfEdges) {
    if (e.source === id && e.sourceHandle) connectedHandles.add(e.sourceHandle);
    if (e.target === id && e.targetHandle) connectedHandles.add(e.targetHandle);
  }

  const handleClass = (portId: string) =>
    `w-[9px] h-[9px] bg-bg-input border-[1.5px] rounded-full cursor-crosshair transition-all duration-150 hover:bg-accent hover:scale-130 [&.connectingto]:bg-green [&.connectingto]:border-green [&.valid]:bg-green [&.valid]:border-green${connectedHandles.has(portId) ? ' connected' : ''}`;

  return (
    <CanvasErrorTooltip message={errorMessage}>
      <div
        className="nowheel widget-card-acrylic rounded-md min-w-[140px] max-w-[220px] text-[11px] relative [&.selected]:border-accent"
        style={errorMessage ? { boxShadow: '0 0 0 2px #ef4444' } : undefined}
      >
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
      {/* out 字节口 + 数值帧端口 (右): 预设 ch0..chN / custom 命名端口 */}
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
        {ports.map((port) => {
          // RawData 预设的 str 口是字符串域 (其余端口均为时域数值口)
          const isStr = rawData && port === 'str';
          const color = isStr ? STRING_DOMAIN_COLOR : TIME_DOMAIN_COLOR;
          return (
            <div key={port} className="flex items-center gap-1 h-[14px] relative pr-0.5" title={`${port} · ${t(lang, isStr ? 'domainString' : 'domainTime')}`}>
              <span className="w-[5px] h-[5px] rounded-full flex-shrink-0 pointer-events-none" style={{ backgroundColor: color }} />
              <span className="text-[9px] text-text-secondary font-mono whitespace-nowrap bg-bg-sidebar px-0.5 py-px rounded-sm">{port}</span>
              <Handle
                type="source"
                position={Position.Right}
                id={port}
                style={{ position: 'relative', right: 'auto', top: 'auto', transform: 'none', borderColor: color }}
                className={handleClass(port)}
              />
            </div>
          );
        })}
      </div>
      {/* 内容占位: 通道数摘要 (RawData 无数值通道, 显示 str 口) */}
      <div className="px-2 py-1.5 text-[10px] font-mono text-text-secondary">
        {rawData ? 'str' : `${channels} ${t(lang, 'channels')}`}
        {nodeData.convertTo ? ` → ${t(lang, protocolLabelKey[nodeData.convertTo.kind] ?? nodeData.convertTo.kind)}` : ''}
      </div>
    </div>
    </CanvasErrorTooltip>
  );
});
