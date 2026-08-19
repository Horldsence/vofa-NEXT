// ============ 协议帧 schema (与 Rust vofa-next-core schema.rs 严格对齐) ============
//
// 协议 = 一份帧 schema: decode 块列表 (解析) + 可选 encode 块列表 (编码)。
// 所有现有协议 kind (JustFloat/FireWater/RawData/Slcan/CandleLight/LogicDecode)
// 都是 schema 的预设; 用户编辑块后 preset='custom', 走后端 SchemaEngine。
//
// serde 约定 (Rust 端注解):
// - ProtocolSchema: camelCase (preset/legacyConfig/decode/encode)
// - SchemaPreset:   camelCase 字符串 ("justFloat"/"fireWater"/.../"custom")
// - DecoderBlock:   tag="type" 字段平铺 (见 frameDecoder.ts)
// - EncodeBlock:    tag="type" content="params" (字段在 params 子对象内, camelCase)

import type { ProtocolConfig } from './transport';
import type { DecoderBlock, ChecksumType, FieldType } from './frameDecoder';

/// schema 预设 — 与 Rust SchemaPreset 对应 (serde rename_all="camelCase")
export type SchemaPreset =
  | 'justFloat'
  | 'fireWater'
  | 'rawData'
  | 'slcan'
  | 'candleLight'
  | 'logicDecode'
  /// 用户自定义块
  | 'custom';

/// 编码块定义 — 与 Rust EncodeBlockDef 对应 (serde tag="type" content="params" + camelCase)
///
/// 注意: 与前端 CommandBlock 形状不同 (CommandBlock 为平铺 snake_case 块类型),
/// 两者不强行复用; 需要互转时在 lib/utils 中建映射函数。
export type EncodeBlock =
  | {
      /// 固定字节序列 (HEX 字符串, 如 "AA BB")
      type: 'constHex';
      params: { hex: string };
    }
  | {
      /// 引用某端口的运行时值, 按 fieldType 编码
      type: 'varRef';
      params: { portName: string; fieldType: FieldType };
    }
  | {
      /// 字面量常量, 按 fieldType 编码 (value 为十进制/浮点字符串)
      type: 'typedConst';
      params: { value: string; fieldType: FieldType };
    }
  | {
      /// 对前序累计字节计算校验并追加
      type: 'checksum';
      params: { algorithm: ChecksumType; customScript?: string };
    };

/// 协议帧 schema — 与 Rust ProtocolSchema 对应 (serde camelCase)
///
/// - preset:       预设标识; 'custom' = 用户自定义块 (走后端 SchemaEngine)
/// - legacyConfig: 预设对应的 legacy 引擎配置 (custom 时为 null/省略)
///                 Rust 端 skip_serializing_if = None → 省略与 null 等价
/// - decode:       解析方向块列表
/// - encode:       编码方向块列表 (预设省略 = 走 legacy 编码)
export interface ProtocolSchema {
  preset: SchemaPreset;
  legacyConfig?: ProtocolConfig | null;
  decode: DecoderBlock[];
  encode?: EncodeBlock[] | null;
}
