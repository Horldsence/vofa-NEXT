//! 帧解码块定义 — FieldType + DecoderBlockDef + 辅助枚举。
//!
//! 与前端 `DecoderBlock` TS 联合类型对应 (serde tag="type" + camelCase)。

use serde::{Deserialize, Serialize};

use logic_types::LogicDecoderConfig;

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
    pub const fn byte_len(self) -> Option<usize> {
        match self {
            Self::UInt8 | Self::Int8 => Some(1),
            Self::UInt16LE | Self::UInt16BE | Self::Int16LE | Self::Int16BE => Some(2),
            Self::UInt32LE
            | Self::UInt32BE
            | Self::Int32LE
            | Self::Int32BE
            | Self::Float32LE
            | Self::Float32BE => Some(4),
            Self::Bytes => None,
        }
    }

    /// 从字节切片解析为 f32 (按字段类型解码)
    /// 长度不足时返回 None
    // 字节编解码本质就是有损/截断数值转换 (u32→f32 精度、i8 回绕等), 语义有意为之
    #[allow(
        clippy::cast_possible_wrap,
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub fn decode(self, bytes: &[u8]) -> Option<f32> {
        match self {
            Self::UInt8 => bytes.first().map(|&b| f32::from(b)),
            Self::Int8 => bytes.first().map(|&b| f32::from(b as i8)),
            Self::UInt16LE => {
                if bytes.len() < 2 {
                    return None;
                }
                Some(f32::from(u16::from_le_bytes([bytes[0], bytes[1]])))
            }
            Self::UInt16BE => {
                if bytes.len() < 2 {
                    return None;
                }
                Some(f32::from(u16::from_be_bytes([bytes[0], bytes[1]])))
            }
            Self::Int16LE => {
                if bytes.len() < 2 {
                    return None;
                }
                Some(f32::from(i16::from_le_bytes([bytes[0], bytes[1]])))
            }
            Self::Int16BE => {
                if bytes.len() < 2 {
                    return None;
                }
                Some(f32::from(i16::from_be_bytes([bytes[0], bytes[1]])))
            }
            Self::UInt32LE => {
                if bytes.len() < 4 {
                    return None;
                }
                Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f32)
            }
            Self::UInt32BE => {
                if bytes.len() < 4 {
                    return None;
                }
                Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f32)
            }
            Self::Int32LE => {
                if bytes.len() < 4 {
                    return None;
                }
                Some((i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])) as f32)
            }
            Self::Int32BE => {
                if bytes.len() < 4 {
                    return None;
                }
                Some((i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])) as f32)
            }
            Self::Float32LE => {
                if bytes.len() < 4 {
                    return None;
                }
                Some(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            }
            Self::Float32BE => {
                if bytes.len() < 4 {
                    return None;
                }
                Some(f32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            }
            Self::Bytes => {
                // Bytes 类型输出第一字节 (作为数值预览), 长度由 length_ref 决定
                bytes.first().map(|&b| f32::from(b))
            }
        }
    }

    /// 按字段类型把 f32 值编码为字节 (编码方向, EncodeBlockDef 用)
    ///
    /// 整型按截断转换; Bytes 类型无固定长度, 编码为单字节 (低 8 位)。
    // 与 decode 同理: 截断/符号转换是编码语义的预期行为
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    pub fn encode(self, value: f32) -> Vec<u8> {
        match self {
            Self::UInt8 => vec![value as u8],
            Self::Int8 => vec![(value as i8) as u8],
            Self::UInt16LE => (value as u16).to_le_bytes().to_vec(),
            Self::UInt16BE => (value as u16).to_be_bytes().to_vec(),
            Self::Int16LE => (value as i16).to_le_bytes().to_vec(),
            Self::Int16BE => (value as i16).to_be_bytes().to_vec(),
            Self::UInt32LE => (value as u32).to_le_bytes().to_vec(),
            Self::UInt32BE => (value as u32).to_be_bytes().to_vec(),
            Self::Int32LE => (value as i32).to_le_bytes().to_vec(),
            Self::Int32BE => (value as i32).to_be_bytes().to_vec(),
            Self::Float32LE => value.to_le_bytes().to_vec(),
            Self::Float32BE => value.to_be_bytes().to_vec(),
            Self::Bytes => vec![value as u8],
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

/// ASCII 字段的进制 (AsciiField 块用)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AsciiBase {
    Hex,
    Dec,
}

/// 帧解码块定义 (与前端 DecoderBlock 对应, serde tag="type" + camelCase)
///
/// 使用 `tag = "type"` (无 content) 模式: 每个 variant 的所有字段直接在对象顶层,
/// 与前端 DecoderBlock 结构一致 (id/type/fieldType/portName/... 同级)。
///
/// 每个块都有 `id` 字段 (前端生成的唯一标识, 用于 length_ref 引用)。
/// 每个块可选 `match_id` 字段 (Id 块除外) — 仅当当前帧的 id_value 等于 match_id 时该块执行。
/// 未设置 match_id 的块始终执行 (用于多帧类型分派)。
///
/// 扩展块 (schema 模型新增, 协议引擎 SchemaEngine 使用):
/// - Csv:        FireWater 类分隔符文本帧 (一行 = 一帧, 按 separator 切分到各端口)
/// - AsciiField: Slcan 类 ASCII 定宽字段 (按进制解析 digits 个字符)
/// - Samples:    逻辑解码采样块 (LogicDecode 类, 整块委托给逻辑解码器)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
        algorithm: super::ChecksumAlgorithm,
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
    /// 分隔符文本帧 (FireWater 类): 一行 = 一帧, 按 separator 切分, 逐列解析为 f32
    /// 输出到 ports 各端口 (列数多于 ports 时忽略多余列, 少于时缺失端口不输出)
    Csv {
        /// 列分隔符 (如 ",")
        separator: String,
        /// 各列输出端口名 (按列序)
        ports: Vec<String>,
    },
    /// ASCII 定宽字段 (Slcan 类): 读 digits 个 ASCII 字符按进制解析为整数, 输出到 port_name
    AsciiField {
        /// 输出端口名
        port_name: String,
        /// 进制 (hex / dec)
        base: AsciiBase,
        /// 字符数 (定宽)
        digits: usize,
    },
    /// 逻辑解码采样块 (LogicDecode 类): 字节流整体喂入逻辑解码器,
    /// 输出 LogicSample / DecodedEvent 而非 DataFrame 通道
    Samples { decoder: LogicDecoderConfig },
}

impl DecoderBlockDef {
    /// 返回块的 id (扩展块 Csv/AsciiField/Samples 无 id, 返回空串)
    pub fn id(&self) -> &str {
        match self {
            Self::Header { id, .. }
            | Self::Length { id, .. }
            | Self::Id { id, .. }
            | Self::Field { id, .. }
            | Self::Bitfield { id, .. }
            | Self::Checksum { id, .. }
            | Self::Tail { id, .. } => id,
            Self::Csv { .. } | Self::AsciiField { .. } | Self::Samples { .. } => "",
        }
    }

    /// 返回该块的 match_id (Id 块与扩展块返回 None)
    pub const fn match_id(&self) -> Option<i64> {
        match self {
            Self::Header { match_id, .. }
            | Self::Length { match_id, .. }
            | Self::Field { match_id, .. }
            | Self::Bitfield { match_id, .. }
            | Self::Checksum { match_id, .. }
            | Self::Tail { match_id, .. } => *match_id,
            Self::Id { .. } | Self::Csv { .. } | Self::AsciiField { .. } | Self::Samples { .. } => {
                None
            }
        }
    }

    /// 返回该块的输出端口名 (有输出端口的块: Length/Id/Field/Bitfield/AsciiField)
    /// Header/Checksum/Tail/Csv(多端口, 见 ports)/Samples 无单一输出端口, 返回 None
    /// Length 默认 "length", Id 默认 "id_value"
    pub fn output_port_name(&self) -> Option<&str> {
        match self {
            Self::Length { port_name, .. } => Some(port_name.as_deref().unwrap_or("length")),
            Self::Id { port_name, .. } => Some(port_name.as_deref().unwrap_or("id_value")),
            Self::Field { port_name, .. } => Some(port_name.as_str()),
            Self::Bitfield { port_name, .. } => Some(port_name.as_str()),
            Self::AsciiField { port_name, .. } => Some(port_name.as_str()),
            Self::Header { .. }
            | Self::Checksum { .. }
            | Self::Tail { .. }
            | Self::Csv { .. }
            | Self::Samples { .. } => None,
        }
    }
}
