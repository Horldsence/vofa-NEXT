import { memo } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { Cable, X } from 'lucide-react';
import type { TransportNodeData } from '../../store/appStoreHelpers';
import type { TransportConfig } from '../../types';

/// 字节域端口颜色 (与 WidgetNode.domainColor 一致)
export const BYTES_DOMAIN_COLOR = '#e5c07b';

/// 连接状态徽标颜色
const STATE_DOT: Record<string, string> = {
  Disconnected: 'bg-text-muted',
  Connecting: 'bg-yellow animate-pulse',
  Connected: 'bg-green',
  Error: 'bg-red',
};

/// 传输配置摘要 (单行)
function configSummary(config: TransportConfig): string {
  switch (config.kind) {
    case 'Serial':
      return `${config.params.port_name || '—'} @ ${config.params.baud_rate}`;
    case 'Slcan':
      return `${config.params.port_name || '—'} @ ${config.params.baud_rate}`;
    case 'Udp':
      return `${config.params.local_addr}:${config.params.local_port} → ${config.params.remote_addr}:${config.params.remote_port}`;
    case 'TcpClient':
      return `${config.params.host}:${config.params.port}`;
    case 'TcpServer':
      return `${config.params.listen_addr}:${config.params.listen_port}`;
    case 'TestData':
      return `${config.params.channels}ch @ ${config.params.sample_rate}Hz ${config.params.signal}`;
    case 'CandleLight':
      return `bus${config.params.bus} ch${config.params.channel}`;
  }
}

const kindLabelKey: Record<TransportConfig['kind'], string> = {
  Serial: 'serial',
  Udp: 'udp',
  TcpClient: 'tcpClient',
  TcpServer: 'tcpServer',
  TestData: 'testData',
  Slcan: 'slcan',
  CandleLight: 'candleLight',
};

/// 数据接口 (Transport) 全局节点 — 字节平面
/// 输出端口 rx (接收字节流), 输入端口 tx (发送字节流)
export const TransportNode = memo(function TransportNode({ id, data }: NodeProps) {
  const lang = useAppStore((s) => s.lang);
  const removeGlobalNode = useAppStore((s) => s.removeGlobalNode);
  const connectionState = useAppStore((s) => s.connectionStates[id] ?? 'Disconnected');
  const rfEdges = useAppStore((s) => s.rfEdges);
  const config = (data as unknown as TransportNodeData).config;

  const connectedHandles = new Set<string>();
  for (const e of rfEdges) {
    if (e.source === id && e.sourceHandle) connectedHandles.add(e.sourceHandle);
    if (e.target === id && e.targetHandle) connectedHandles.add(e.targetHandle);
  }

  const handleClass = (portId: string) =>
    `w-[9px] h-[9px] bg-bg-input border-[1.5px] rounded-full cursor-crosshair transition-all duration-150 hover:bg-accent hover:scale-130 [&.connectingto]:bg-green [&.connectingto]:border-green [&.valid]:bg-green [&.valid]:border-green${connectedHandles.has(portId) ? ' connected' : ''}`;

  return (
    <div className="nowheel widget-card-acrylic rounded-md min-w-[150px] max-w-[220px] text-[11px] relative [&.selected]:border-accent">
      <div className="flex items-center justify-between px-1.5 py-1 border-b border-border text-[10px] font-semibold uppercase tracking-[0.4px] text-yellow">
        <span className="flex items-center gap-1 flex-1 truncate">
          <Cable size={11} />
          {t(lang, kindLabelKey[config.kind])}
        </span>
        <span
          className={`w-2 h-2 rounded-full inline-block flex-shrink-0 ${STATE_DOT[connectionState]}`}
          title={connectionState}
        />
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
      <div className="px-2 py-1.5 text-[10px] font-mono text-text-secondary truncate" title={configSummary(config)}>
        {configSummary(config)}
      </div>
      {/* tx 输入口 (左) / rx 输出口 (右) — 均为字节域 */}
      <div className="absolute top-1/2 left-0 -translate-y-1/2 flex flex-col gap-0.5 py-1">
        <div className="flex items-center gap-1 h-[14px] relative pl-0.5" title={`tx · ${t(lang, 'domainBytes')}`}>
          <Handle
            type="target"
            position={Position.Left}
            id="tx"
            style={{ position: 'relative', left: 'auto', top: 'auto', transform: 'none', borderColor: BYTES_DOMAIN_COLOR }}
            className={handleClass('tx')}
          />
          <span className="text-[9px] text-text-secondary font-mono whitespace-nowrap bg-bg-sidebar px-0.5 py-px rounded-sm">tx</span>
          <span className="w-[5px] h-[5px] rounded-full flex-shrink-0 pointer-events-none" style={{ backgroundColor: BYTES_DOMAIN_COLOR }} />
        </div>
      </div>
      <div className="absolute top-1/2 right-0 -translate-y-1/2 flex flex-col items-end gap-0.5 py-1 z-10">
        <div className="flex items-center gap-1 h-[14px] relative pr-0.5" title={`rx · ${t(lang, 'domainBytes')}`}>
          <span className="w-[5px] h-[5px] rounded-full flex-shrink-0 pointer-events-none" style={{ backgroundColor: BYTES_DOMAIN_COLOR }} />
          <span className="text-[9px] text-text-secondary font-mono whitespace-nowrap bg-bg-sidebar px-0.5 py-px rounded-sm">rx</span>
          <Handle
            type="source"
            position={Position.Right}
            id="rx"
            style={{ position: 'relative', right: 'auto', top: 'auto', transform: 'none', borderColor: BYTES_DOMAIN_COLOR }}
            className={handleClass('rx')}
          />
        </div>
      </div>
    </div>
  );
});
