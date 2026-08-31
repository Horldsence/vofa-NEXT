import type { PortSampleStatus } from './sampleProtocol';

/** 数值平面中一个可订阅输出端口的稳定身份。 */
export interface NumericPortRef {
  readonly nodeId: string;
  readonly handle: string;
  readonly domain: 'time';
}

/** 保留后端序号与采样时间的真实数值样本。 */
export interface NumericSample {
  readonly seq: number;
  readonly ts: number;
  readonly value: number;
}

/** React 显示层统一消费的端口状态；latest=null 与真实 value=0 严格区分。 */
export interface NumericPortState {
  readonly source: NumericPortRef | null;
  readonly connected: boolean;
  readonly legacyBinding: boolean;
  readonly status: PortSampleStatus;
  readonly latest: NumericSample | null;
  readonly history: readonly NumericSample[];
  readonly previewSkipped: number;
  readonly retentionEvicted: number;
  readonly ingressDropped: number;
  readonly error: string | null;
}

export function numericPortRef(nodeId: string, handle: string): NumericPortRef {
  return { nodeId, handle, domain: 'time' };
}
