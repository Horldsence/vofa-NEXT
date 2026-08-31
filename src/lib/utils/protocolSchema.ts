/// 协议帧 schema 预设工厂 + 端口派生 (与 Rust schema.rs 对齐)
///
/// 预设 (JustFloat/FireWater/RawData/Slcan/CandleLight/LogicDecode) 是 schema 的
/// 工厂产物: preset = 对应值 + legacyConfig = 原 ProtocolConfig + decode 块生成
/// ch0..chN 端口 (RawData 例外 — 引擎不产数值帧, 端口为 str 字符串口, 见 protocolPortNames)。
/// 预设路径仍走 legacy 引擎 (自动检测/CAN/逻辑事件能力保留);
/// 用户编辑块后 preset='custom', 端口由 decode 块派生 (见 schemaPortNames)。

import type {
  DecoderBlock,
  LogicDecoderConfig,
  ProtocolConfig,
  ProtocolSchema,
} from '../../types';

/// 预设默认端口数 (自动检测生效前的占位, 端口语义仍走 legacy 自动检测)
export const DEFAULT_PRESET_CHANNELS = 4;

/// JustFloat 帧尾 (0x00 0x00 0x80 0x7F)
const JUSTFLOAT_TAIL_HEX = '00 00 80 7F';

/// 获取当前生效通道数 (优先检测值, 其次配置值)
/// 注: 本函数原在 store/appStoreHelpers, 移至此处避免循环依赖 (appStoreHelpers 仍 re-export)
/// RawData/Slcan/CandleLight/LogicDecode 返回占位 4 — 这些引擎不产数值帧,
/// 节点端口由 protocolPortNames 另行决定 (RawData → ['str']), 该值仅作 data.channels 占位
export function getEffectiveChannels(
  protocolConfig: ProtocolConfig,
  detectedChannels: number | null
): number {
  if (protocolConfig.kind === 'RawData' || protocolConfig.kind === 'Slcan' || protocolConfig.kind === 'CandleLight' || protocolConfig.kind === 'LogicDecode') return DEFAULT_PRESET_CHANNELS;
  const configured = protocolConfig.channels;
  if (configured != null) return configured;
  return detectedChannels ?? DEFAULT_PRESET_CHANNELS;
}

/// JustFloat 预设: n×field{float32LE, portName ch{i}} + tail{00 00 80 7F}
/// channels=null (自动) 也生成默认 4 端口, 端口语义仍走 legacy 自动检测
export function justfloatSchema(channels: number | null): ProtocolSchema {
  const n = channels ?? DEFAULT_PRESET_CHANNELS;
  const decode: DecoderBlock[] = [];
  for (let i = 0; i < n; i++) {
    decode.push({ id: `jf-f${i}`, type: 'field', fieldType: 'float32LE', portName: `ch${i}` });
  }
  decode.push({ id: 'jf-tail', type: 'tail', hex: JUSTFLOAT_TAIL_HEX });
  return {
    preset: 'justFloat',
    legacyConfig: { kind: 'JustFloat', channels },
    decode,
  };
}

/// FireWater 预设: csv 块 (逗号分隔, 列 → ch0..chN 端口)
export function firewaterSchema(channels: number | null): ProtocolSchema {
  const n = channels ?? DEFAULT_PRESET_CHANNELS;
  const ports = Array.from({ length: n }, (_, i) => `ch${i}`);
  return {
    preset: 'fireWater',
    legacyConfig: { kind: 'FireWater', channels },
    decode: [{ type: 'csv', separator: ',', ports }],
  };
}

/// RawData 预设: 原始字节透传, 无 decode 块
export function rawDataSchema(): ProtocolSchema {
  return { preset: 'rawData', legacyConfig: { kind: 'RawData' }, decode: [] };
}

/// Slcan 预设: CAN 帧走 legacy 引擎, 无 decode 块
export function slcanSchema(): ProtocolSchema {
  return { preset: 'slcan', legacyConfig: { kind: 'Slcan' }, decode: [] };
}

/// CandleLight 预设: CAN 帧走 legacy 引擎, 无 decode 块
export function candleLightSchema(): ProtocolSchema {
  return { preset: 'candleLight', legacyConfig: { kind: 'CandleLight' }, decode: [] };
}

/// LogicDecode 预设: samples 块委托给逻辑解码器
export function logicDecodeSchema(decoder: LogicDecoderConfig): ProtocolSchema {
  return {
    preset: 'logicDecode',
    legacyConfig: { kind: 'LogicDecode', decoder },
    decode: [{ type: 'samples', decoder }],
  };
}

/// 旧 ProtocolConfig → 预设 schema (节点创建 / 快照迁移用)
export function schemaFromProtocolConfig(config: ProtocolConfig): ProtocolSchema {
  switch (config.kind) {
    case 'JustFloat':
      return justfloatSchema(config.channels);
    case 'FireWater':
      return firewaterSchema(config.channels);
    case 'RawData':
      return rawDataSchema();
    case 'Slcan':
      return slcanSchema();
    case 'CandleLight':
      return candleLightSchema();
    case 'LogicDecode':
      return logicDecodeSchema(config.decoder);
  }
}

/// 从 decode 块派生端口列表 (与 Rust ProtocolSchema::port_names 规则一致):
/// field.portName / bitfield.portName / csv.ports / asciiField.portName,
/// 按块顺序去重 (保持首次出现顺序)。length/id/checksum/header/tail/samples 不占端口。
export function schemaPortNames(decode: DecoderBlock[]): string[] {
  const ports: string[] = [];
  for (const b of decode) {
    if (b.type === 'field' || b.type === 'bitfield' || b.type === 'asciiField') {
      if (!ports.includes(b.portName)) ports.push(b.portName);
    } else if (b.type === 'csv') {
      for (const p of b.ports) {
        if (!ports.includes(p)) ports.push(p);
      }
    }
  }
  return ports;
}

/// RawData 预设判定 (覆盖有/无 schema 两种数据形态; custom 块编辑后不再是预设)
/// RawData 引擎不产数值帧: 收到字节后 out(Bytes) 透传 + UTF-8 lossy 文本进 str 字符串口
export function isRawDataPreset(nodeData: { config: ProtocolConfig; schema?: ProtocolSchema | null }): boolean {
  const schema = nodeData.schema;
  if (schema?.preset === 'custom') return false;
  return schema?.preset === 'rawData' || nodeData.config.kind === 'RawData';
}

/// 协议节点输出口名字列表:
/// - custom schema → 按 port_names 规则从 decode 块派生 (任意数量、可命名)
/// - RawData 预设 → 单个 str 字符串口 (无 chN 数值口)
/// - 其他预设 (或缺失 schema 的旧数据) → 现有 getEffectiveChannels 逻辑产 ch0..chN
export function protocolPortNames(
  nodeData: { config: ProtocolConfig; channels: number; schema?: ProtocolSchema | null },
  detectedChannels: number | null
): string[] {
  const schema = nodeData.schema;
  if (schema?.preset === 'custom') return schemaPortNames(schema.decode);
  if (isRawDataPreset(nodeData)) return ['str'];
  const n = nodeData.channels > 0
    ? nodeData.channels
    : getEffectiveChannels(nodeData.config, detectedChannels);
  return Array.from({ length: n }, (_, i) => `ch${i}`);
}
