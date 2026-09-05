import { memo } from 'react';
import { useAppStore } from '../../../store/appStore';
import { t } from '../../../i18n';
import type { WidgetConfig } from '../../../types';
import { resolveRawDataStatusTransport } from '../../../lib/utils/rawDataChannel';

/// RawData 卡片的源连接状态提示 — 生效输入端口的 Transport 未连接时灰字提示,
/// Error 红字 (无法正确使用); Connected 不显示 (绿点噪音)。
/// 无连线 / FrameDecoder raw 口 (无固定连接语义) 也不显示。
const RawDataConnHint = memo(function RawDataConnHint({ nodeId }: { nodeId: string }) {
  const lang = useAppStore((s) => s.lang);
  const selectedInput = useAppStore((s) => {
    const w = s.widgets.find((w) => w.kind === 'RawData' && w.params.id === nodeId);
    return w?.kind === 'RawData' ? w.params.selectedInput : undefined;
  });
  const rfNodes = useAppStore((s) => s.rfNodes);
  const widgets = useAppStore((s) => s.widgets);
  const rfEdges = useAppStore((s) => s.rfEdges);
  const transportId = resolveRawDataStatusTransport(nodeId, selectedInput, rfEdges, rfNodes, widgets);
  const connState = useAppStore((s) =>
    transportId ? (s.connectionStates[transportId] ?? 'Disconnected') : null
  );
  if (!transportId || connState === 'Connected' || connState === null) return null;
  const isError = connState === 'Error';
  return (
    <span
      className={`flex items-center gap-1 text-[9px] ${
        isError ? 'text-red' : 'text-text-secondary'
      }`}
      title={t(lang, isError ? 'connError' : 'notConnected')}
    >
      <span
        className={`w-1.5 h-1.5 rounded-full flex-shrink-0 ${
          isError ? 'bg-red' : 'bg-text-muted'
        }`}
      />
      {t(lang, isError ? 'connError' : 'notConnected')}
    </span>
  );
});

interface NodePlaceholderProps {
  kind: WidgetConfig['kind'];
  nodeId: string;
  showRawDataHint?: boolean;
}

/// 窗口型控件的节点内占位 — 实际渲染在数据窗口 (双击节点打开/激活)。
/// WidgetNode 根部的双击处理负责开窗, 这里只负责提示与 RawData 连接状态。
export const NodePlaceholder = memo(function NodePlaceholder({ kind, nodeId, showRawDataHint = false }: NodePlaceholderProps) {
  const lang = useAppStore((s) => s.lang);
  return (
    <div className="flex flex-col items-center gap-1 px-2 py-3 text-text-secondary text-[10px] text-center">
      <span>{kind}</span>
      <span className="text-blue text-[9px]">↗ {t(lang, 'nodeOpenWindowHint')}</span>
      {showRawDataHint && kind === 'RawData' && <RawDataConnHint nodeId={nodeId} />}
    </div>
  );
});
