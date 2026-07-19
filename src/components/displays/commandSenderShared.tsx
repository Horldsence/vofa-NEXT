import {
  Hexagon,
  Variable,
  Hash,
  ShieldCheck,
} from 'lucide-react';
import type {
  BlockType,
  CommandBlock,
  ChecksumType,
  FieldType,
} from '../../types';

/// 块类型配置: 图标 / Tailwind 静态类名 / 标签 key
/// 颜色全部使用 @theme 内已定义的语义色 (blue/green/yellow/red)
export const BLOCK_TYPE_CONFIG: Record<
  BlockType,
  {
    icon: React.ReactNode;
    badgeClass: string;   // 标签: bg-{c}/20 text-{c} border-{c}/40
    blockClass: string;   // 块容器: border-{c}/40 bg-{c}/10
    iconClass: string;    // 图标: text-{c}
    labelKey: string;
    addLabelKey: string;
  }
> = {
  const_hex:   { icon: <Hexagon size={12} />,     badgeClass: 'bg-blue/20 text-blue border-blue/40',       blockClass: 'border-blue/40 bg-blue/10',       iconClass: 'text-blue',   labelKey: 'cmdBlockConstHex',   addLabelKey: 'cmdAddBlockConstHex' },
  var_ref:     { icon: <Variable size={12} />,    badgeClass: 'bg-green/20 text-green border-green/40',   blockClass: 'border-green/40 bg-green/10',     iconClass: 'text-green',  labelKey: 'cmdBlockVarRef',     addLabelKey: 'cmdAddBlockVarRef' },
  typed_const: { icon: <Hash size={12} />,        badgeClass: 'bg-yellow/20 text-yellow border-yellow/40', blockClass: 'border-yellow/40 bg-yellow/10',   iconClass: 'text-yellow', labelKey: 'cmdBlockTypedConst', addLabelKey: 'cmdAddBlockTypedConst' },
  checksum:    { icon: <ShieldCheck size={12} />, badgeClass: 'bg-red/20 text-red border-red/40',         blockClass: 'border-red/40 bg-red/10',         iconClass: 'text-red',    labelKey: 'cmdBlockChecksum',   addLabelKey: 'cmdAddBlockChecksum' },
};

export const CHECKSUM_OPTIONS: { value: ChecksumType; labelKey: string }[] = [
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

/// 拼接多个 Uint8Array
export function concatChunks(chunks: Uint8Array[]): Uint8Array {
  const total = chunks.reduce((s, c) => s + c.length, 0);
  const result = new Uint8Array(total);
  let offset = 0;
  for (const c of chunks) {
    result.set(c, offset);
    offset += c.length;
  }
  return result;
}

/// 块摘要 (列表中单行显示)
export function blockSummary(block: CommandBlock): string {
  switch (block.type) {
    case 'const_hex':
      return block.hex ?? '';
    case 'var_ref':
      return `${block.portName ?? 'value'} : ${block.fieldType ?? 'uint16LE'}`;
    case 'typed_const':
      return `${block.value ?? '0'} : ${block.fieldType ?? 'uint8'}`;
    case 'checksum':
      return block.checksum ?? 'sum8';
  }
}
