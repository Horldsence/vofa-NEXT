//! 帧 schema 模型 — 协议 = 一份帧 schema (块列表)
//!
//! 解析是帧解码 (decode 块列表), 生成/发送是帧编码 (encode 块列表),
//! 共用同一份定义。所有现有协议 kind (JustFloat/FireWater/RawData/Slcan/
//! CandleLight/LogicDecode) 都是 schema 的预设 ([`SchemaPreset`]);
//! 用户可自定义块 (Custom)。
//!
//! 本模块集中存放跨 crate 共享的纯数据类型:
//! - [`DecoderBlockDef`] 等解码块类型 (自 vofa-next-nodes 迁入, nodes 做 re-export)
//! - [`ChecksumAlgorithm`] 校验算法 (自 vofa-next-nodes 迁入, 含纯函数求值)
//! - [`ProtocolSchema`] / [`SchemaPreset`] / [`EncodeBlockDef`] 新增 schema 定义
//!
//! serde 约定与前端 TS 类型一一对应 (camelCase; DecoderBlockDef 为 tag="type"
//! 字段平铺, EncodeBlockDef 为 tag="type" content="params")。

use serde::{Deserialize, Serialize};

use crate::logic::LogicDecoderConfig;

// ============ 校验算法 ============

/// 校验算法 (与前端 ChecksumType 对齐, serde rename 显式指定字符串)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChecksumAlgorithm {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "sum8")]
    Sum8,
    #[serde(rename = "xor8")]
    Xor8,
    #[serde(rename = "crc8")]
    Crc8,
    #[serde(rename = "crc16Modbus")]
    Crc16Modbus,
    #[serde(rename = "crc16CCITT")]
    Crc16CCITT,
    #[serde(rename = "crc32")]
    Crc32,
    #[serde(rename = "lrc")]
    Lrc,
    #[serde(rename = "custom")]
    Custom,
}

impl ChecksumAlgorithm {
    /// 计算校验值 (返回单字节或 4 字节, 由调用方截取)
    pub fn compute(self, data: &[u8], custom_script: Option<&str>) -> Vec<u8> {
        match self {
            Self::None => Vec::new(),
            Self::Sum8 => {
                let s: u8 = data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
                vec![s]
            }
            Self::Xor8 => {
                let x: u8 = data.iter().fold(0u8, |acc, &b| acc ^ b);
                vec![x]
            }
            Self::Crc8 => vec![crc8(data, 0x07, 0x00, 0x00)],
            Self::Crc16Modbus => {
                let crc = crc16_modbus(data);
                crc.to_le_bytes().to_vec()
            }
            Self::Crc16CCITT => {
                let crc = crc16_ccitt(data);
                crc.to_be_bytes().to_vec()
            }
            Self::Crc32 => {
                let crc = crc32(data);
                crc.to_le_bytes().to_vec()
            }
            Self::Lrc => {
                let lrc: u8 = data.iter().fold(0u8, |acc, &b| acc.wrapping_sub(b));
                vec![lrc]
            }
            Self::Custom => {
                // 自定义脚本暂不支持后端求值 (前端 lib/checksum.ts 中的 customChecksum 用 JS 实现)
                // 后端此处返回空 Vec, 实际项目应引入 rhai/boa 等 JS 引擎求值
                let _ = custom_script;
                Vec::new()
            }
        }
    }

    /// 比较计算值与期望值 (自动处理长度差异)
    pub fn verify(self, data: &[u8], expected: &[u8], custom_script: Option<&str>) -> bool {
        let computed = self.compute(data, custom_script);
        if computed.is_empty() {
            return true; // None / Custom 未实现 → 默认通过
        }
        computed == expected
    }

    /// 校验算法输出的字节长度
    pub const fn byte_len(self) -> usize {
        match self {
            Self::None => 0,
            Self::Sum8
            | Self::Xor8
            | Self::Crc8
            | Self::Lrc => 1,
            Self::Crc16Modbus | Self::Crc16CCITT => 2,
            Self::Crc32 => 4,
            Self::Custom => 0, // Custom 暂不支持后端求值
        }
    }
}

// ============ CRC 算法实现 ============

/// CRC-8 (poly=0x07, init=0x00, refin=false, refout=false, xorout=0x00)
fn crc8(data: &[u8], poly: u8, init: u8, xorout: u8) -> u8 {
    let mut crc = init;
    for &b in data {
        crc ^= b;
        for _ in 0..8 {
            if crc & 0x80 != 0 {
                crc = (crc << 1) ^ poly;
            } else {
                crc <<= 1;
            }
        }
    }
    crc ^ xorout
}

/// CRC-16 Modbus (poly=0x8005, init=0xFFFF, refin=true, refout=true, xorout=0x0000)
fn crc16_modbus(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc ^= u16::from(b);
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xA001; // 0x8005 反转
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}

/// CRC-16 CCITT (poly=0x1021, init=0xFFFF, refin=false, refout=false, xorout=0x0000)
fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc ^= u16::from(b) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

/// CRC-32 (poly=0x04C11DB7, init=0xFFFFFFFF, refin=true, refout=true, xorout=0xFFFFFFFF)
fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320; // 0x04C11DB7 反转
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

// ============ 解码块类型 (自 vofa-next-nodes 迁入) ============

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
            Self::UInt16LE | Self::UInt16BE | Self::Int16LE | Self::Int16BE => {
                Some(2)
            }
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
            Self::Csv { .. }
            | Self::AsciiField { .. }
            | Self::Samples { .. } => "",
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
            Self::Id { .. }
            | Self::Csv { .. }
            | Self::AsciiField { .. }
            | Self::Samples { .. } => None,
        }
    }

    /// 返回该块的输出端口名 (有输出端口的块: Length/Id/Field/Bitfield/AsciiField)
    /// Header/Checksum/Tail/Csv(多端口, 见 ports)/Samples 无单一输出端口, 返回 None
    /// Length 默认 "length", Id 默认 "id_value"
    pub fn output_port_name(&self) -> Option<&str> {
        match self {
            Self::Length { port_name, .. } => {
                Some(port_name.as_deref().unwrap_or("length"))
            }
            Self::Id { port_name, .. } => {
                Some(port_name.as_deref().unwrap_or("id_value"))
            }
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

// ============ 帧 schema ============

/// schema 预设 — 所有现有协议 kind 都是 schema 的预设
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SchemaPreset {
    JustFloat,
    FireWater,
    RawData,
    Slcan,
    CandleLight,
    LogicDecode,
    /// 用户自定义块
    Custom,
}

/// 协议帧 schema — 解析 (decode) 与编码 (encode) 共用同一份定义
///
/// serde camelCase, 与前端 TS `ProtocolSchema` 类型对应。
///
/// PartialEq 为手工实现: legacy_config (ProtocolConfig) 未派生 PartialEq,
/// 沿用应用层惯例用 serde 值比较。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolSchema {
    pub preset: SchemaPreset,
    /// 预设对应的 legacy 引擎配置 (Custom 为 None) — 预设引擎构造用
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_config: Option<crate::config::ProtocolConfig>,
    /// 解析方向块列表
    #[serde(default)]
    pub decode: Vec<DecoderBlockDef>,
    /// 编码方向块列表 (TestData 生成 / 协议转换用; 预设可为 None = 走 legacy 编码)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encode: Option<Vec<EncodeBlockDef>>,
}

impl PartialEq for ProtocolSchema {
    fn eq(&self, other: &Self) -> bool {
        self.preset == other.preset
            && self.decode == other.decode
            && self.encode == other.encode
            && serde_json::to_value(&self.legacy_config).ok()
                == serde_json::to_value(&other.legacy_config).ok()
    }
}

impl ProtocolSchema {
    /// 从 decode 块派生端口列表 (前后端一致的规则):
    /// field.portName / bitfield.portName / csv.ports / asciiField.portName
    /// 按块顺序组成端口列表 (去重, 保持首次出现顺序)。
    ///
    /// 预设 (非 Custom) 端口为 ch0..chN (N 来自 legacy_config.channels 或自动检测),
    /// 不走本派生 — 保持现有行为。
    pub fn port_names(&self) -> Vec<String> {
        let mut ports: Vec<String> = Vec::new();
        for b in &self.decode {
            match b {
                DecoderBlockDef::Field { port_name, .. }
                | DecoderBlockDef::Bitfield { port_name, .. }
                | DecoderBlockDef::AsciiField { port_name, .. } => {
                    if !ports.contains(port_name) {
                        ports.push(port_name.clone());
                    }
                }
                DecoderBlockDef::Csv { ports: ps, .. } => {
                    for p in ps {
                        if !ports.contains(p) {
                            ports.push(p.clone());
                        }
                    }
                }
                _ => {}
            }
        }
        ports
    }
}

/// 编码块定义 (镜像前端 CommandBlock, serde tag="type" content="params" + camelCase)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "type",
    content = "params",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum EncodeBlockDef {
    /// 固定字节序列 (HEX 字符串, 如 "AA BB")
    ConstHex { hex: String },
    /// 引用某端口的运行时值, 按 field_type 编码
    VarRef {
        port_name: String,
        field_type: FieldType,
    },
    /// 字面量常量, 按 field_type 编码 (value 为十进制/浮点字符串)
    TypedConst { value: String, field_type: FieldType },
    /// 对前序累计字节计算校验并追加
    Checksum {
        algorithm: ChecksumAlgorithm,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        custom_script: Option<String>,
    },
}

/// 按 encode 块列表编码一帧 (SchemaEngine 编码 / TestData 生成共用)
///
/// - `ports`: 端口名列表 (由 schema decode 块派生), VarRef 按名字索引 values
/// - `values`: 与 ports 对齐的运行时值 (越界/缺失按 0.0)
pub fn encode_by_blocks(encode: &[EncodeBlockDef], ports: &[String], values: &[f32]) -> Vec<u8> {
    let mut out = Vec::new();
    for b in encode {
        match b {
            EncodeBlockDef::ConstHex { hex } => {
                out.extend_from_slice(&parse_hex(hex));
            }
            EncodeBlockDef::VarRef {
                port_name,
                field_type,
            } => {
                let v = ports
                    .iter()
                    .position(|p| p == port_name)
                    .and_then(|i| values.get(i))
                    .copied()
                    .unwrap_or(0.0);
                out.extend_from_slice(&field_type.encode(v));
            }
            EncodeBlockDef::TypedConst { value, field_type } => {
                let v: f32 = value.trim().parse().unwrap_or(0.0);
                out.extend_from_slice(&field_type.encode(v));
            }
            EncodeBlockDef::Checksum {
                algorithm,
                custom_script,
            } => {
                let cs = algorithm.compute(&out, custom_script.as_deref());
                out.extend_from_slice(&cs);
            }
        }
    }
    out
}

/// 测试数据链路配置 — TestData 生成器热更新载荷
///
/// 兼容旧的 `ProtocolConfig` 调用方: schema 为 None 或预设时走 legacy 编码;
/// schema 为 Custom 且带 encode 块时按 schema 编码。
#[derive(Debug, Clone)]
pub struct TestDataLink {
    pub protocol: crate::config::ProtocolConfig,
    pub schema: Option<ProtocolSchema>,
}

impl TestDataLink {
    pub const fn new(protocol: crate::config::ProtocolConfig) -> Self {
        Self {
            protocol,
            schema: None,
        }
    }
}

// ============ HEX 解析工具 ============

/// 解析 HEX 字符串为字节切片
///
/// 输入格式: "AA BB" / "AABB" / "aa bb" / "0xAA 0xBB" 均可,
/// 空格/逗号/0x 前缀均会被忽略。
///
/// 解析失败 (奇数长度 / 非法字符) 返回空 Vec。
pub fn parse_hex(hex: &str) -> Vec<u8> {
    // 过滤空白与逗号, 并移除所有 "0x" 前缀 (允许 "0xAA 0xBB" 格式)
    let cleaned: String = hex
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',')
        .collect();
    let cleaned = cleaned.replace("0x", "");
    if !cleaned.len().is_multiple_of(2) {
        return Vec::new();
    }
    (0..cleaned.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&cleaned[i..i + 2], 16).ok())
        .collect::<Option<Vec<u8>>>()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProtocolConfig;

    /// 构造 JustFloat 等价的自定义 schema (4×float32LE field + tail)
    fn justfloat_like_schema() -> ProtocolSchema {
        ProtocolSchema {
            preset: SchemaPreset::Custom,
            legacy_config: None,
            decode: vec![
                DecoderBlockDef::Field {
                    id: "f0".into(),
                    field_type: FieldType::Float32LE,
                    port_name: "a".into(),
                    length_ref: None,
                    match_id: None,
                },
                DecoderBlockDef::Field {
                    id: "f1".into(),
                    field_type: FieldType::Float32LE,
                    port_name: "b".into(),
                    length_ref: None,
                    match_id: None,
                },
                DecoderBlockDef::Tail {
                    id: "t0".into(),
                    hex: "00 00 80 7F".into(),
                    match_id: None,
                },
            ],
            encode: Some(vec![
                EncodeBlockDef::VarRef {
                    port_name: "a".into(),
                    field_type: FieldType::Float32LE,
                },
                EncodeBlockDef::VarRef {
                    port_name: "b".into(),
                    field_type: FieldType::Float32LE,
                },
                EncodeBlockDef::ConstHex {
                    hex: "00 00 80 7F".into(),
                },
            ]),
        }
    }

    #[test]
    fn test_schema_serde_roundtrip() {
        let schema = justfloat_like_schema();
        let json = serde_json::to_value(&schema).unwrap();
        // camelCase 字段名
        assert!(json.get("legacyConfig").is_some() || schema.legacy_config.is_none());
        // 预设为 camelCase 字符串
        assert_eq!(json["preset"], serde_json::json!("custom"));
        // 解码块: tag="type" 字段平铺
        assert_eq!(json["decode"][0]["type"], serde_json::json!("field"));
        assert_eq!(
            json["decode"][0]["fieldType"],
            serde_json::json!("float32LE")
        );
        assert_eq!(json["decode"][0]["portName"], serde_json::json!("a"));
        // 编码块: tag="type" content="params"
        assert_eq!(json["encode"][0]["type"], serde_json::json!("varRef"));
        assert_eq!(
            json["encode"][0]["params"]["portName"],
            serde_json::json!("a")
        );
        assert_eq!(
            json["encode"][2]["type"],
            serde_json::json!("constHex")
        );

        let back: ProtocolSchema = serde_json::from_value(json).unwrap();
        assert_eq!(back, schema);
    }

    #[test]
    fn test_schema_new_block_variants_serde() {
        let schema = ProtocolSchema {
            preset: SchemaPreset::Custom,
            legacy_config: None,
            decode: vec![
                DecoderBlockDef::Csv {
                    separator: ",".into(),
                    ports: vec!["x".into(), "y".into()],
                },
                DecoderBlockDef::AsciiField {
                    port_name: "id".into(),
                    base: AsciiBase::Hex,
                    digits: 3,
                },
                DecoderBlockDef::Samples {
                    decoder: LogicDecoderConfig::Uart {
                        baud_rate: 115200,
                        data_bits: 8,
                        parity: crate::config::Parity::None,
                        stop_bits: crate::config::StopBits::One,
                        channel: 0,
                    },
                },
            ],
            encode: None,
        };
        let json = serde_json::to_value(&schema).unwrap();
        assert_eq!(json["decode"][0]["type"], serde_json::json!("csv"));
        assert_eq!(json["decode"][1]["type"], serde_json::json!("asciiField"));
        assert_eq!(json["decode"][1]["base"], serde_json::json!("hex"));
        assert_eq!(json["decode"][2]["type"], serde_json::json!("samples"));
        let back: ProtocolSchema = serde_json::from_value(json).unwrap();
        assert_eq!(back, schema);
    }

    #[test]
    fn test_schema_preset_serde() {
        // 预设枚举 camelCase
        assert_eq!(
            serde_json::to_value(SchemaPreset::JustFloat).unwrap(),
            serde_json::json!("justFloat")
        );
        assert_eq!(
            serde_json::to_value(SchemaPreset::CandleLight).unwrap(),
            serde_json::json!("candleLight")
        );
        assert_eq!(
            serde_json::to_value(SchemaPreset::LogicDecode).unwrap(),
            serde_json::json!("logicDecode")
        );
    }

    #[test]
    fn test_schema_with_legacy_config_roundtrip() {
        let schema = ProtocolSchema {
            preset: SchemaPreset::JustFloat,
            legacy_config: Some(ProtocolConfig::JustFloat { channels: Some(4) }),
            decode: vec![],
            encode: None,
        };
        let json = serde_json::to_string(&schema).unwrap();
        let back: ProtocolSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(back, schema);
    }

    #[test]
    fn test_port_names_derivation() {
        let schema = ProtocolSchema {
            preset: SchemaPreset::Custom,
            legacy_config: None,
            decode: vec![
                DecoderBlockDef::Header {
                    id: "h".into(),
                    hex: "AA".into(),
                    match_id: None,
                },
                DecoderBlockDef::Field {
                    id: "f0".into(),
                    field_type: FieldType::UInt8,
                    port_name: "v0".into(),
                    length_ref: None,
                    match_id: None,
                },
                DecoderBlockDef::Bitfield {
                    id: "b0".into(),
                    byte_offset: 1,
                    bit_offset: 0,
                    bit_length: 4,
                    is_signed: false,
                    port_name: "flags".into(),
                    match_id: None,
                },
                DecoderBlockDef::Csv {
                    separator: ",".into(),
                    ports: vec!["c0".into(), "c1".into()],
                },
                DecoderBlockDef::AsciiField {
                    port_name: "hex_id".into(),
                    base: AsciiBase::Hex,
                    digits: 2,
                },
            ],
            encode: None,
        };
        assert_eq!(schema.port_names(), vec!["v0", "flags", "c0", "c1", "hex_id"]);
    }

    #[test]
    fn test_encode_by_blocks() {
        let schema = justfloat_like_schema();
        let ports = schema.port_names();
        let bytes = encode_by_blocks(
            schema.encode.as_ref().unwrap(),
            &ports,
            &[1.0, 2.0],
        );
        let mut expect = Vec::new();
        expect.extend_from_slice(&1.0f32.to_le_bytes());
        expect.extend_from_slice(&2.0f32.to_le_bytes());
        expect.extend_from_slice(&[0x00, 0x00, 0x80, 0x7F]);
        assert_eq!(bytes, expect);
    }

    #[test]
    fn test_encode_by_blocks_checksum() {
        let encode = vec![
            EncodeBlockDef::ConstHex { hex: "01 02".into() },
            EncodeBlockDef::Checksum {
                algorithm: ChecksumAlgorithm::Sum8,
                custom_script: None,
            },
        ];
        let bytes = encode_by_blocks(&encode, &[], &[]);
        assert_eq!(bytes, vec![0x01, 0x02, 0x03]);
    }

    #[test]
    fn test_field_type_encode_roundtrip() {
        for ft in [
            FieldType::UInt8,
            FieldType::Int8,
            FieldType::UInt16LE,
            FieldType::UInt16BE,
            FieldType::Int16LE,
            FieldType::Int16BE,
            FieldType::UInt32LE,
            FieldType::UInt32BE,
            FieldType::Int32LE,
            FieldType::Int32BE,
            FieldType::Float32LE,
            FieldType::Float32BE,
        ] {
            let v = 42.0f32;
            let bytes = ft.encode(v);
            assert_eq!(ft.decode(&bytes), Some(v), "{ft:?} 编解码应往返");
        }
    }
}
