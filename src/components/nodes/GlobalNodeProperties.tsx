import { memo } from 'react';
import type { Node } from '@xyflow/react';
import { useAppStore } from '../../store/appStore';
import { t } from '../../i18n';
import { Plug, PlugZap, Play, Square } from 'lucide-react';
import { TransportConfigForm } from '../panels/transport/TransportConfigForm';
import { ProtocolConfigForm } from '../panels/protocol/ProtocolConfigForm';
import { useConnectAction } from '../panels/transport/useConnectAction';
import { isRawDataPreset } from '../../lib/utils/protocolSchema';
import { downstreamProtocolOf, type ProtocolNodeData, type TransportNodeData } from '../../store/appStoreHelpers';

/// Transport 节点属性 — 配置表单 + 连接/断开 + TestData 启停
const TransportProperties = memo(function TransportProperties({ node }: { node: Node }) {
  const lang = useAppStore((s) => s.lang);
  const setTransportNodeConfig = useAppStore((s) => s.setTransportNodeConfig);
  const disconnectNode = useAppStore((s) => s.disconnectNode);
  const connectionState = useAppStore((s) => s.connectionStates[node.id] ?? 'Disconnected');
  const testDataRunning = useAppStore((s) => s.testDataRunning[node.id] ?? false);
  const startTestData = useAppStore((s) => s.startTestData);
  const stopTestData = useAppStore((s) => s.stopTestData);
  const { state: connectState, formAction: connectAction, isPending: connectPending } = useConnectAction(node.id);

  const config = (node.data as unknown as TransportNodeData).config;
  const isConnected = connectionState === 'Connected';
  const isTestData = config.kind === 'TestData';

  // TestData 提示: 下游 Protocol 节点协议名
  const downstreamId = useAppStore((s) => downstreamProtocolOf(node.id, s.rfEdges, s.rfNodes));
  const downstreamConfig = useAppStore((s) => {
    if (!downstreamId) return null;
    const n = s.rfNodes.find((x) => x.id === downstreamId);
    return n ? (n.data as unknown as ProtocolNodeData).config : null;
  });

  return (
    <div>
      <TransportConfigForm
        value={config}
        onChange={(c) => setTransportNodeConfig(node.id, c)}
        lang={lang}
        protocolLabel={downstreamConfig?.kind}
      />

      {/* TestData 开始/停止控制 */}
      {isTestData && (
        <div className="mb-3 p-2.5 bg-bg-input rounded border border-border">
          <div className="flex items-center justify-between mb-1.5">
            <span className="text-xs font-medium text-text-secondary">{t(lang, 'testData')}</span>
            <span className={`text-xs ${isConnected ? 'text-text-secondary' : 'text-text-disabled'}`}>
              {isConnected
                ? testDataRunning
                  ? t(lang, 'testDataRunning')
                  : t(lang, 'testDataStopped')
                : t(lang, 'notConnected')}
            </span>
          </div>
          {testDataRunning ? (
            <button
              type="button"
              onClick={() => { void stopTestData(node.id); }}
              disabled={!isConnected}
              className="w-full px-3 h-8 bg-bg-danger text-text-bright border-none rounded cursor-pointer text-sm text-center transition-colors hover:bg-bg-danger-hover inline-flex items-center justify-center gap-1.5 disabled:opacity-50 disabled:cursor-default"
            >
              <Square size={14} />
              {t(lang, 'stopTestData')}
            </button>
          ) : (
            <button
              type="button"
              onClick={() => { void startTestData(node.id); }}
              disabled={!isConnected}
              className="w-full px-3 h-8 bg-bg-button text-text-inverse border-none rounded cursor-pointer text-sm text-center transition-colors hover:bg-bg-button-hover inline-flex items-center justify-center gap-1.5 disabled:opacity-50 disabled:cursor-default"
            >
              <Play size={14} />
              {t(lang, 'startTestData')}
            </button>
          )}
        </div>
      )}

      {/* 连接控制 */}
      <div className="mt-3 pt-2 border-t border-border" data-tour="connect">
        {isConnected ? (
          <button
            className="w-full px-3 h-8 bg-bg-danger text-text-bright border-none rounded cursor-pointer text-sm text-center transition-colors hover:bg-bg-danger-hover inline-flex items-center justify-center gap-1.5"
            onClick={() => { void disconnectNode(node.id); }}
          >
            <PlugZap size={14} />
            {t(lang, 'disconnect')}
          </button>
        ) : (
          <form action={connectAction}>
            <button
              type="submit"
              className="w-full px-3 h-8 bg-bg-button text-text-inverse border-none rounded cursor-pointer text-sm text-center transition-colors hover:bg-bg-button-hover inline-flex items-center justify-center gap-1.5 disabled:opacity-50 disabled:cursor-default"
              disabled={connectPending || connectionState === 'Connecting'}
            >
              <Plug size={14} />
              {connectPending ? t(lang, 'connecting') : t(lang, 'connect')}
            </button>
            {connectState.error && (
              <div className="mt-1.5 text-xs text-red-400 text-center break-all">{connectState.error}</div>
            )}
          </form>
        )}
      </div>
    </div>
  );
});

/// Protocol 节点属性 — 协议配置 + 可选协议转换 (convert_to)
const ProtocolProperties = memo(function ProtocolProperties({ node }: { node: Node }) {
  const lang = useAppStore((s) => s.lang);
  const setProtocolNodeConfig = useAppStore((s) => s.setProtocolNodeConfig);
  const setProtocolNodeConvertTo = useAppStore((s) => s.setProtocolNodeConvertTo);
  const setProtocolNodeSchema = useAppStore((s) => s.setProtocolNodeSchema);
  const detectedChannels = useAppStore((s) => s.detectedChannels[node.id] ?? null);

  const data = node.data as unknown as ProtocolNodeData;
  const convertTo = data.convertTo ?? null;

  const convertKinds: { value: string; label: string }[] = [
    { value: '', label: t(lang, 'convertNone') },
    { value: 'JustFloat', label: t(lang, 'justfloat') },
    { value: 'FireWater', label: t(lang, 'firewater') },
    { value: 'RawData', label: t(lang, 'rawdata') },
  ];

  const onConvertKindChange = (kind: string) => {
    if (!kind) {
      setProtocolNodeConvertTo(node.id, null);
      return;
    }
    const prev = convertTo;
    if (kind === 'JustFloat' || kind === 'FireWater') {
      const channels =
        prev && (prev.kind === 'JustFloat' || prev.kind === 'FireWater') ? prev.channels : null;
      setProtocolNodeConvertTo(node.id, { kind, channels });
    } else {
      setProtocolNodeConvertTo(node.id, { kind: 'RawData' });
    }
  };

  return (
    <div>
      <ProtocolConfigForm
        value={data.config}
        onChange={(c) => void setProtocolNodeConfig(node.id, c)}
        lang={lang}
        detectedChannels={detectedChannels}
        schema={data.schema}
        onSchemaChange={(s) => setProtocolNodeSchema(node.id, s)}
      />

      {/* 协议转换 (convert_to) — 默认无转换 */}
      <div className="mt-3 pt-2 border-t border-border">
        <label className="block text-xs text-text-secondary mb-1">{t(lang, 'convertTo')}</label>
        <select
          value={convertTo?.kind ?? ''}
          onChange={(e) => onConvertKindChange(e.target.value)}
          className="form-select"
        >
          {convertKinds.map((k) => (
            <option key={k.value} value={k.value}>{k.label}</option>
          ))}
        </select>
        {/* RawData 预设不产帧: 提示 convert_to 不生效、原文透传 (后端语义) */}
        {isRawDataPreset(data) && (
          <div className="mt-1 text-[10px] text-text-secondary opacity-80 break-all">
            {t(lang, 'convertRawDataHint')}
          </div>
        )}
        {convertTo && (convertTo.kind === 'JustFloat' || convertTo.kind === 'FireWater') && (
          <div className="mt-2">
            <ProtocolConfigForm
              value={convertTo}
              onChange={(c) => setProtocolNodeConvertTo(node.id, c)}
              lang={lang}
              nameSuffix="-convert"
            />
          </div>
        )}
      </div>
    </div>
  );
});

/// 全局节点属性面板 — NodeEditor 右侧, 选中 Transport/Protocol 节点时显示
export const GlobalNodeProperties = memo(function GlobalNodeProperties({ node }: { node: Node }) {
  const lang = useAppStore((s) => s.lang);
  return (
    <div className="absolute top-2 right-2 bottom-2 w-[260px] z-20 bg-bg-sidebar border border-border rounded-md shadow-lg overflow-y-auto p-3">
      <div className="text-[10px] font-semibold uppercase tracking-wider text-text-secondary mb-2">
        {node.type === 'transport' ? t(lang, 'dataInterface') : t(lang, 'protocolEngine')}
      </div>
      {node.type === 'transport' ? <TransportProperties node={node} /> : <ProtocolProperties node={node} />}
    </div>
  );
});
