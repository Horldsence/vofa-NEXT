//! # schema_types
//!
//! 帧 schema 数据类型 — 协议 = 一份帧 schema (块列表)。
//!
//! 解析是帧解码 (decode 块列表), 生成/发送是帧编码 (encode 块列表),
//! 共用同一份定义。所有现有协议 kind (JustFloat/FireWater/RawData/Slcan/
//! CandleLight/LogicDecode) 都是 schema 的预设 ([`SchemaPreset`]);
//! 用户可自定义块 (Custom)。
//!
//! 本 crate 集中存放跨 crate 共享的纯数据类型:
//! - [`ChecksumAlgorithm`] 校验算法及 CRC/SUM/XOR 求值
//! - [`FieldType`] / [`DecoderBlockDef`] / [`EncodeBlockDef`] 解码/编码块定义
//! - [`ProtocolSchema`] / [`SchemaPreset`] / [`ProtocolConfig`] schema 配置
//! - [`parse_hex`] HEX 字符串解析工具
//!
//! serde 约定与前端 TS 类型一一对应 (camelCase; DecoderBlockDef 为 tag="type"
//! 字段平铺, EncodeBlockDef 为 tag="type" content="params")。
//!
//! ## 设计原则
//!
//! 1. **零 I/O 依赖**:不引入 tokio/serialport,纯数据结构。
//! 2. **serde 优先**:所有 wire 类型派生 `Serialize`/`Deserialize`,与前端 IPC。
//! 3. **单职责**:本 crate 承载协议 schema 完整数据模型,不含调度/解析逻辑。

pub mod checksum;
pub mod decoder_block;
pub mod hex;
pub mod protocol_config;
pub mod schema_frame;

pub use checksum::ChecksumAlgorithm;
pub use decoder_block::{
    AsciiBase, DecoderBlockDef, DecoderChecksumCover, DecoderChecksumPosition, FieldType,
    LengthUnit,
};
pub use hex::parse_hex;
pub use protocol_config::ProtocolConfig;
pub use schema_frame::{
    encode_by_blocks, EncodeBlockDef, ProtocolSchema, SchemaPreset, TestDataLink,
};
