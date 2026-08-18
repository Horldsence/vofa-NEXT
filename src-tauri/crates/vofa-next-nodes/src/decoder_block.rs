//! 帧解码块类型定义 (FrameDecoder 的块列表元素)
//!
//! 与前端 `DecoderBlock` 类型对齐 (src/types/index.ts),
//! serde 使用 `tag = "type"` 与前端 discriminant 字段 "type" 一致。
//!
//! 块类型:
//! - Header:   匹配帧头固定字节序列 (帧起始标志)
//! - Length:   读 N 字节为整数, 输出到 length 端口 + 决定后续变长字段长度
//! - Id:       读 N 字节为整数, 输出到 id_value 端口 + 设置 match_id 上下文
//! - Field:    按 field_type 读 N 字节并解码为 f32, 输出到 port_name 端口
//! - Bitfield: 从指定字节按 bit 偏移+位长读取, 输出到 port_name 端口
//! - Checksum: 对前序累计字节校验, 输出 valid 端口 (1.0/0.0)
//! - Tail:     匹配帧尾固定字节序列 (可选, 帧结束标志)

use serde::{Deserialize, Serialize};

use crate::frame_decoder::ChecksumAlgorithm;

/// 整数字段类型 (与前端 FieldType 对应)
///
/// serde rename_all="kebab-case" 与前端 PascalCase 不同 —
/// 这里使用 serde rename 显式指定每个变体的字符串, 确保与前端 TS 联合类型字符串完全一致。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FieldType {
    #[serde(rename = "uint8")]
    UInt8,
    #[serde(rename = "int8")]
    Int8,
    #[serde(rename = "uint16LE")]
    UInt16LE,
    #[serde(rename = "uint16BE")]
    UInt16BE,
    #[serde(rename = "int16LE")]
    Int16LE,
    #[serde(rename = "int16BE")]
    Int16BE,
    #[serde(rename = "uint32LE")]
    UInt32LE,
    #[serde(rename = "uint32BE")]
    UInt32BE,
    #[serde(rename = "int32LE")]
    Int32LE,
    #[serde(rename = "int32BE")]
    Int32BE,
    #[serde(rename = "float32LE")]
    Float32LE,
    #[serde(rename = "float32BE")]
    Float32BE,
    /// 变长字节序列 (长度由 length_ref 决定)
    #[serde(rename = "bytes")]
    Bytes,
}

impl FieldType {
    /// 该字段类型的固定字节长度 (Bytes 返回 None, 需由 length_ref 决定)
    pub fn byte_len(self) -> Option<usize> {
        match self {
            FieldType::UInt8 | FieldType::Int8 => Some(1),
            FieldType::UInt16LE | FieldType::UInt16BE | FieldType::Int16LE | FieldType::Int16BE => {
                Some(2)
            }
            FieldType::UInt32LE
            | FieldType::UInt32BE
            | FieldType::Int32LE
            | FieldType::Int32BE
            | FieldType::Float32LE
            | FieldType::Float32BE => Some(4),
            FieldType::Bytes => None,
        }
    }

    /// 从字节切片解析为 f32 (按字段类型解码)
    /// 长度不足时返回 None
    pub fn decode(self, bytes: &[u8]) -> Option<f32> {
        match self {
            FieldType::UInt8 => bytes.first().map(|&b| b as f32),
            FieldType::Int8 => bytes.first().map(|&b| (b as i8) as f32),
            FieldType::UInt16LE => {
                if bytes.len() < 2 {
                    return None;
                }
                Some(u16::from_le_bytes([bytes[0], bytes[1]]) as f32)
            }
            FieldType::UInt16BE => {
                if bytes.len() < 2 {
                    return None;
                }
                Some(u16::from_be_bytes([bytes[0], bytes[1]]) as f32)
            }
            FieldType::Int16LE => {
                if bytes.len() < 2 {
                    return None;
                }
                Some((i16::from_le_bytes([bytes[0], bytes[1]])) as f32)
            }
            FieldType::Int16BE => {
                if bytes.len() < 2 {
                    return None;
                }
                Some((i16::from_be_bytes([bytes[0], bytes[1]])) as f32)
            }
            FieldType::UInt32LE => {
                if bytes.len() < 4 {
                    return None;
                }
                Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f32)
            }
            FieldType::UInt32BE => {
                if bytes.len() < 4 {
                    return None;
                }
                Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f32)
            }
            FieldType::Int32LE => {
                if bytes.len() < 4 {
                    return None;
                }
                Some((i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])) as f32)
            }
            FieldType::Int32BE => {
                if bytes.len() < 4 {
                    return None;
                }
                Some((i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])) as f32)
            }
            FieldType::Float32LE => {
                if bytes.len() < 4 {
                    return None;
                }
                Some(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            }
            FieldType::Float32BE => {
                if bytes.len() < 4 {
                    return None;
                }
                Some(f32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            }
            FieldType::Bytes => {
                // Bytes 类型输出第一字节 (作为数值预览), 长度由 length_ref 决定
                bytes.first().map(|&b| b as f32)
            }
        }
    }
}

/// 帧解码块的覆盖范围 (校验计算的字节范围)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecoderChecksumCover {
    /// 从帧开头到本校验块之前的所有字节
    AllPrior,
    /// 用户指定字节偏移范围 [cover_start, cover_end)
    Range,
}

/// 帧解码校验位置
/// - Append:  校验字节位于帧末尾 (在 tail 之前)
/// - Inline:  校验字节位于当前位置 (在块列表中该 checksum 块的位置)
/// - Prepend: 校验字节位于帧头之后 (在 header 之后)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecoderChecksumPosition {
    Append,
    Inline,
    Prepend,
}

/// 长度块的单位
/// - Bytes:  字节数 (length 值表示后续字段的字节长度)
/// - Fields: 后续 field 块重复次数 (length 值表示后续 field 块重复 N 次)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LengthUnit {
    Bytes,
    Fields,
}

/// 帧解码块定义 (与前端 DecoderBlock 对应, serde tag="type" + camelCase)
///
/// 使用 `tag = "type"` (无 content) 模式: 每个 variant 的所有字段直接在对象顶层,
/// 与前端 DecoderBlock 结构一致 (id/type/fieldType/portName/... 同级)。
///
/// 每个块都有 `id` 字段 (前端生成的唯一标识, 用于 length_ref 引用)。
/// 每个块可选 `match_id` 字段 (Id 块除外) — 仅当当前帧的 id_value 等于 match_id 时该块执行。
/// 未设置 match_id 的块始终执行 (用于多帧类型分派)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DecoderBlockDef {
    /// 帧头: 匹配固定字节序列 (帧起始标志)
    Header {
        /// 块 id (前端生成, 用于 UI 引用)
        id: String,
        /// HEX 字符串, 如 "AA BB" (空格可选)
        hex: String,
        /// 可选 match_id (用于多帧类型分派)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        match_id: Option<i64>,
    },
    /// 长度字段: 读 N 字节为整数, 输出到 length 端口 + 决定后续变长字段长度
    Length {
        id: String,
        field_type: FieldType,
        /// 输出端口名 (默认 "length")
        #[serde(default, skip_serializing_if = "Option::is_none")]
        port_name: Option<String>,
        /// 长度单位 (默认 bytes)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unit: Option<LengthUnit>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        match_id: Option<i64>,
    },
    /// 帧类型 ID: 读 N 字节为整数, 输出到 id_value 端口 + 设置 match_id 上下文
    Id {
        id: String,
        field_type: FieldType,
        /// 输出端口名 (默认 "id_value")
        #[serde(default, skip_serializing_if = "Option::is_none")]
        port_name: Option<String>,
    },
    /// 数据字段: 按 field_type 读 N 字节并解码为 f32, 输出到 port_name 端口
    Field {
        id: String,
        field_type: FieldType,
        /// 输出端口名 (节点上暴露的 Handle id)
        port_name: String,
        /// 若设置, 引用某个 Length 块的 id — 该字段读取 length_value 字节而非 field_type 固定长度
        /// (仅 field_type=Bytes 时生效, 输出第一字节为 f32; 其他类型忽略此字段)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        length_ref: Option<String>,
        /// 仅当 id_value === match_id 时执行 (多帧分派)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        match_id: Option<i64>,
    },
    /// 位域字段: 从指定字节按 bit 偏移+位长读取, 输出到 port_name 端口
    Bitfield {
        id: String,
        /// 字节偏移 (相对于帧头之后的位置)
        byte_offset: u32,
        /// 位偏移 (0-7)
        bit_offset: u8,
        /// 位长度 (1-32)
        bit_length: u8,
        /// 是否带符号 (true=最高位为符号位, 二补码)
        is_signed: bool,
        /// 输出端口名
        port_name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        match_id: Option<i64>,
    },
    /// 校验: 对前序累计字节校验, 输出 valid 端口 (1.0/0.0)
    Checksum {
        id: String,
        /// 校验算法
        algorithm: ChecksumAlgorithm,
        /// 自定义脚本 (algorithm=Custom 时使用)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        custom_script: Option<String>,
        /// 校验覆盖范围
        cover: DecoderChecksumCover,
        /// cover=Range 时的起始字节偏移 (相对帧头之后)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cover_start: Option<u32>,
        /// cover=Range 时的结束字节偏移 (exclusive)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cover_end: Option<u32>,
        /// 校验字节在帧中的位置
        position: DecoderChecksumPosition,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        match_id: Option<i64>,
    },
    /// 帧尾: 匹配固定字节序列 (可选, 帧结束标志)
    Tail {
        id: String,
        /// HEX 字符串
        hex: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        match_id: Option<i64>,
    },
}

impl DecoderBlockDef {
    /// 返回块的 id
    pub fn id(&self) -> &str {
        match self {
            DecoderBlockDef::Header { id, .. }
            | DecoderBlockDef::Length { id, .. }
            | DecoderBlockDef::Id { id, .. }
            | DecoderBlockDef::Field { id, .. }
            | DecoderBlockDef::Bitfield { id, .. }
            | DecoderBlockDef::Checksum { id, .. }
            | DecoderBlockDef::Tail { id, .. } => id,
        }
    }

    /// 返回该块的 match_id (Id 块返回 None)
    pub fn match_id(&self) -> Option<i64> {
        match self {
            DecoderBlockDef::Header { match_id, .. }
            | DecoderBlockDef::Length { match_id, .. }
            | DecoderBlockDef::Field { match_id, .. }
            | DecoderBlockDef::Bitfield { match_id, .. }
            | DecoderBlockDef::Checksum { match_id, .. }
            | DecoderBlockDef::Tail { match_id, .. } => *match_id,
            DecoderBlockDef::Id { .. } => None,
        }
    }

    /// 返回该块的输出端口名 (有输出端口的块: Length/Id/Field/Bitfield)
    /// Header/Checksum/Tail 无输出端口, 返回 None
    /// Length 默认 "length", Id 默认 "id_value"
    pub fn output_port_name(&self) -> Option<&str> {
        match self {
            DecoderBlockDef::Length { port_name, .. } => {
                Some(port_name.as_deref().unwrap_or("length"))
            }
            DecoderBlockDef::Id { port_name, .. } => {
                Some(port_name.as_deref().unwrap_or("id_value"))
            }
            DecoderBlockDef::Field { port_name, .. } => Some(port_name.as_str()),
            DecoderBlockDef::Bitfield { port_name, .. } => Some(port_name.as_str()),
            DecoderBlockDef::Header { .. }
            | DecoderBlockDef::Checksum { .. }
            | DecoderBlockDef::Tail { .. } => None,
        }
    }
}
