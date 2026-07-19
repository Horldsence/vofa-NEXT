import type {
  DecoderBlock,
  DecoderBlockType,
  DecoderChecksumPosition,
  DecoderChecksumCover,
  ChecksumType,
  FieldType,
  InputFormat,
} from '../../types';
import {
  Hexagon,
  Hash,
  Fingerprint,
  Variable,
  Grid3x3,
  ShieldCheck,
  Flag,
} from 'lucide-react';

/// 块类型配置: 图标 / Tailwind 静态类名 / 标签 key
/// 颜色全部使用 @theme 内已定义的语义色 (blue/yellow/purple/green/orange/red/accent)
export const BLOCK_TYPE_CONFIG: Record<
  DecoderBlockType,
  {
    icon: React.ReactNode;
    badgeClass: string;   // 标签: bg-{c}/20 text-{c} border-{c}/40
    blockClass: string;   // 块容器: border-{c}/40 bg-{c}/10
    iconClass: string;    // 图标: text-{c}
    labelKey: string;
    addLabelKey: string;
  }
> = {
  header:   { icon: <Hexagon size={12} />,     badgeClass: 'bg-blue/20 text-blue border-blue/40',         blockClass: 'border-blue/40 bg-blue/10',         iconClass: 'text-blue',   labelKey: 'fdBlockHeader',   addLabelKey: 'fdAddBlockHeader' },
  length:   { icon: <Hash size={12} />,        badgeClass: 'bg-yellow/20 text-yellow border-yellow/40',   blockClass: 'border-yellow/40 bg-yellow/10',     iconClass: 'text-yellow', labelKey: 'fdBlockLength',   addLabelKey: 'fdAddBlockLength' },
  id:       { icon: <Fingerprint size={12} />, badgeClass: 'bg-purple/20 text-purple border-purple/40',   blockClass: 'border-purple/40 bg-purple/10',     iconClass: 'text-purple', labelKey: 'fdBlockId',       addLabelKey: 'fdAddBlockId' },
  field:    { icon: <Variable size={12} />,    badgeClass: 'bg-green/20 text-green border-green/40',     blockClass: 'border-green/40 bg-green/10',       iconClass: 'text-green',  labelKey: 'fdBlockField',    addLabelKey: 'fdAddBlockField' },
  bitfield: { icon: <Grid3x3 size={12} />,     badgeClass: 'bg-orange/20 text-orange border-orange/40',  blockClass: 'border-orange/40 bg-orange/10',     iconClass: 'text-orange', labelKey: 'fdBlockBitfield', addLabelKey: 'fdAddBlockBitfield' },
  checksum: { icon: <ShieldCheck size={12} />, badgeClass: 'bg-red/20 text-red border-red/40',           blockClass: 'border-red/40 bg-red/10',           iconClass: 'text-red',    labelKey: 'fdBlockChecksum', addLabelKey: 'fdAddBlockChecksum' },
  tail:     { icon: <Flag size={12} />,        badgeClass: 'bg-accent/20 text-accent border-accent/40',  blockClass: 'border-accent/40 bg-accent/10',     iconClass: 'text-accent', labelKey: 'fdBlockTail',     addLabelKey: 'fdAddBlockTail' },
};

export const CHECKSUM_OPTIONS: { value: ChecksumType; labelKey: string }[] = [
  { value: 'none', labelKey: 'cmdChecksumNone' },
  { value: 'sum8', labelKey: 'cmdChecksumSum8' },
  { value: 'xor8', labelKey: 'cmdChecksumXor8' },
  { value: 'crc8', labelKey: 'cmdChecksumCRC8' },
  { value: 'crc16Modbus', labelKey: 'cmdChecksumCRC16Modbus' },
  { value: 'crc16CCITT', labelKey: 'cmdChecksumCRC16CCITT' },
  { value: 'crc32', labelKey: 'cmdChecksumCRC32' },
  { value: 'lrc', labelKey: 'cmdChecksumLRC' },
  { value: 'custom', labelKey: 'cmdChecksumCustom' },
];

export const FIELD_TYPE_OPTIONS: FieldType[] = [
  'uint8', 'int8',
  'uint16LE', 'uint16BE', 'int16LE', 'int16BE',
  'uint32LE', 'uint32BE', 'int32LE', 'int32BE',
  'float32LE', 'float32BE',
  'bytes',
];

export const CHECKSUM_POSITION_OPTIONS: { value: DecoderChecksumPosition; labelKey: string }[] = [
  { value: 'append', labelKey: 'fdChecksumPosAppend' },
  { value: 'inline', labelKey: 'fdChecksumPosInline' },
  { value: 'prepend', labelKey: 'fdChecksumPosPrepend' },
];

export const CHECKSUM_COVER_OPTIONS: { value: DecoderChecksumCover; labelKey: string }[] = [
  { value: 'all_prior', labelKey: 'fdChecksumCoverAllPrior' },
  { value: 'range', labelKey: 'fdChecksumCoverRange' },
];

/// 历史记录最大条数
export const HISTORY_MAX = 20;
/// localStorage key
export const HISTORY_KEY = 'frame-decoder-history-v1';

export interface HistoryEntry {
  input: string;
  format: InputFormat;
  ts: number;
}

/// 示例模板条目 (帧解码器专用, 覆盖典型帧格式)
export interface ExampleEntry {
  name: string;
  description: string;
  format: InputFormat;
  content: string;
}

export const FRAME_EXAMPLES: ExampleEntry[] = [
  {
    name: '定长帧 · header + 2 字段 + sum8',
    description: 'AA 01 02 03 (header=AA, field1=01, field2=02, sum=03)',
    format: 'hex',
    content: 'AA 01 02 03',
  },
  {
    name: '定长帧 · 多帧连续',
    description: 'AA 01 02 03 AA 04 05 09',
    format: 'hex',
    content: 'AA 01 02 03 AA 04 05 09',
  },
  {
    name: '变长帧 · header + length + payload + tail',
    description: 'AA 03 11 22 33 FF (length=3, payload=11 22 33, tail=FF)',
    format: 'hex',
    content: 'AA 03 11 22 33 FF',
  },
  {
    name: '多帧分派 · id 区分帧类型',
    description: 'AA 01 10 20 / AA 02 30 40 50 (id=1 短帧, id=2 长帧)',
    format: 'hex',
    content: 'AA 01 10 20 AA 02 30 40 50',
  },
  {
    name: '带校验 · CRC16-Modbus',
    description: 'AA 01 02 03 04 (含 CRC16, 末尾 2 字节小端)',
    format: 'hex',
    content: 'AA 01 02 0B 6B',
  },
  {
    name: '位域字段 · 单字节多字段',
    description: 'AA 5A 00 (5A = 0101 1010, 高 4 位=5, 低 4 位=A)',
    format: 'hex',
    content: 'AA 5A 00',
  },
];

export function loadHistory(): HistoryEntry[] {
  try {
    const raw = localStorage.getItem(HISTORY_KEY);
    if (!raw) return [];
    const arr = JSON.parse(raw);
    if (!Array.isArray(arr)) return [];
    return arr.filter(
      (e: any) => e && typeof e.input === 'string' && typeof e.format === 'string'
    );
  } catch {
    return [];
  }
}

export function saveHistory(entries: HistoryEntry[]) {
  try {
    localStorage.setItem(HISTORY_KEY, JSON.stringify(entries));
  } catch {
    // 忽略配额错误
  }
}

/// 块摘要 (列表中单行显示)
export function blockSummary(block: DecoderBlock): string {
  switch (block.type) {
    case 'header':
      return block.hex ?? '';
    case 'length':
      return `${block.portName ?? 'length'} : ${block.fieldType ?? 'uint8'}`;
    case 'id':
      return `${block.portName ?? 'id_value'} : ${block.fieldType ?? 'uint8'}`;
    case 'field':
      return `${block.portName ?? ''} : ${block.fieldType ?? 'uint8'}`;
    case 'bitfield':
      return `${block.portName ?? ''} @${block.byteOffset}.${block.bitOffset}:${block.bitLength}${block.isSigned ? 's' : 'u'}`;
    case 'checksum':
      return block.algorithm ?? 'sum8';
    case 'tail':
      return block.hex ?? '';
  }
}

/// 从 blocks 推导输出端口名 (用于 live 模式显示)
export function getOutputPortNames(blocks: DecoderBlock[]): string[] {
  const names: string[] = [];
  for (const b of blocks) {
    if (b.type === 'length') names.push(b.portName ?? 'length');
    else if (b.type === 'id') names.push(b.portName ?? 'id_value');
    else if (b.type === 'field' || b.type === 'bitfield') names.push(b.portName);
  }
  return names;
}
