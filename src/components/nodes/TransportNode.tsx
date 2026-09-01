import { memo } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { Cable, X } from 'lucide-react';
import type { TransportNodeData } from '../../store/appStoreHelpers';
import type { TransportConfig } from '../../types';
import { CanvasErrorTooltip, useCanvasNodeError } from '../ui/CanvasErrorTooltip';

/// 字节域端口颜色 (与 WidgetNode.domainColor 一致)
export const BYTES_DOMAIN_COLOR = '#e5c07b';

/// 连接状态徽标颜色
const STATE_DOT: Record<string, string> = {
  Disconnected: 'bg-text-muted',
  Connecting: 'bg-yellow animate-pulse',
  Connected: 'bg-green',
  Error: 'bg-red',
};

/// 连接状态 → i18n label key (未连接提示 / 错误红字)
const STATE_LABEL_KEY: Record<string, string> = {
  Disconnected: 'notConnected',
  Connecting: 'connecting',
  Connected: 'connected',
  Error: 'connError',
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
  const errorMessage = useCanvasNodeError(id, undefined);
  // 持久高亮 — 与 highlightedNodeId 同步; 错误优先
  const canvasHighlight = useAppStore((s) => s.canvasHighlight);
  const isCanvasHighlighted =
    !!canvasHighlight && canvasHighlight.nodeId === id && !errorMessage;
  const config = (data as unknown as TransportNodeData).config;

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
        className="nowheel widget-card-acrylic rounded-md min-w-[150px] max-w-[220px] text-[11px] relative [&.selected]:border-accent"
        style={
          errorMessage
            ? { boxShadow: '0 0 0 2px #ef4444' }
            : isCanvasHighlighted
              ? { boxShadow: '0 0 0 2px var(--color-accent)' }
              : undefined
        }
      >
      <div className="node-drag-handle flex items-center justify-between px-1.5 py-1 border-b border-border text-[10px] font-semibold uppercase tracking-[0.4px] text-yellow cursor-grab active:cursor-grabbing">
        <span className="flex items-center gap-1 flex-1 truncate">
          <Cable size={11} />
          {t(lang, kindLabelKey[config.kind])}
        </span>
        {/* 未连接/错误文字提示 — Connected 仅保留绿点 (避免常驻噪音), Error 红字 */}
        {connectionState !== 'Connected' && (
          <span
            className={`text-[9px] normal-case tracking-normal flex-shrink-0 mr-1 ${
              connectionState === 'Error' ? 'text-red' : 'text-text-secondary'
            }`}
          >
            {t(lang, STATE_LABEL_KEY[connectionState] ?? 'notConnected')}
          </span>
        )}
        <span
          className={`w-2 h-2 rounded-full inline-block flex-shrink-0 mr-1 ${STATE_DOT[connectionState]}`}
          title={t(lang, STATE_LABEL_KEY[connectionState] ?? 'notConnected')}
        />
        <button
          className="nodrag w-4 h-4 p-0 opacity-60 hover:opacity-100 flex items-center justify-center rounded text-text-secondary hover:bg-bg-hover transition-opacity"
          onClick={(e) => {
            e.stopPropagation();
            removeGlobalNode(id);
          }}
        >
          <X size={10} />
        </button>
      </div>
      
      <div className="flex flex-row w-full min-h-[32px]">
        {/* tx 输入口 (左) */}
        <div className="flex flex-col justify-center gap-0.5 py-1 -ml-1.5 z-10">
          <div className="flex items-center gap-1 h-[14px] relative" title={`tx · ${t(lang, 'domainBytes')}`}>
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

        {/* 内容区 */}
        <div className="flex-1 flex flex-col justify-center px-2 py-1.5 text-[10px] font-mono text-text-secondary truncate text-center" title={configSummary(config)}>
          {configSummary(config)}
        </div>

        {/* rx 输出口 (右) */}
        <div className="flex flex-col items-end justify-center gap-0.5 py-1 -mr-1.5 z-10">
          <div className="flex items-center gap-1 h-[14px] relative" title={`rx · ${t(lang, 'domainBytes')}`}>
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
    </div>
    </CanvasErrorTooltip>
  );
});
