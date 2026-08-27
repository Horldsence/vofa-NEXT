//! SchemaEngine — 自定义帧 schema 的流式协议引擎
//!
//! 协议 = 一份帧 schema (块列表): 解析是帧解码 (decode 块), 发送是帧编码
//! (encode 块), 共用同一份定义。本引擎处理 `SchemaPreset::Custom` 的
//! schema; 预设 schema 由 [`compile_schema`] 分发到 legacy 引擎。
//!
//! 帧定界 / 缓冲策略:
//! - 字节流入内部缓冲 `buf`, 每趟解析循环: 先在缓冲中定位 Header (无 Header
//!   块时从当前位置起解析), 再按 decode 块顺序用 cursor 求值;
//! - 字节不足 (Incomplete) → 保留缓冲等待更多数据; 未匹配到 Header 时仅保留
//!   末尾 header.len()-1 字节 (避免跨包截断), 防止缓冲无限增长 (上限同
//!   JustFloatEngine: 8192 截断到 4096);
//! - 结构错误 (Tail 不匹配 / ASCII 解析失败) → 视为假同步, 丢弃到本帧头之后
//!   重新同步; checksum 校验失败 → 跳过该帧 (不产出 DataFrame) 但消耗字节。
//!
//! 输出: DataFrame.channels 按端口序 (schema.port_names() 派生: field/
//! bitfield/csv/asciiField 块按序), 缺失端口补 0.0。
//!
//! 扩展块:
//! - Csv:        一行 = 一帧, 按 separator 切分列解析为 f32 (FireWater 类)
//! - AsciiField: 定宽 ASCII 字段按进制解析 (Slcan 类)
//! - Samples:    decode 含 Samples 块时整体委托给 LogicDecoderEngine
//!   (LogicDecode 类), 输出 LogicSample / DecodedEvent

pub mod bitfield;
pub mod compile;
pub mod engine;
pub mod protocol_impl;

pub use compile::compile_schema;
pub use engine::SchemaEngine;
pub use protocol_impl::_ensure_protocol_impl_used;
