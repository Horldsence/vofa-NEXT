//! 帧解码块类型定义 (FrameDecoder 的块列表元素)
//!
//! 类型已迁移至 `vofa-next-core::schema` (协议引擎统一为帧 schema 模型,
//! 纯数据类型集中在 core 供 nodes/protocol/transport 共用);
//! 本模块保留 re-export, 现有 `crate::decoder_block::*` 引用不受影响。
//!
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
//! - Csv / AsciiField / Samples: schema 模型扩展块 (见 core::schema)

pub use schema_types::{
    AsciiBase, DecoderBlockDef, DecoderChecksumCover, DecoderChecksumPosition, FieldType,
    LengthUnit,
};
