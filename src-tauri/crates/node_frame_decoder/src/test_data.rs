//! 帧解码器测试数据生成器 (FrameDecoderTestData)
//!
//! 根据 [`DecoderBlockDef`] 配置反向编码字节序列, 使得编码后的字节能够被
//! [`FrameParser`] 解析, 并产生预期的端口输出值。
//!
//! 用于端到端测试帧解码器: 先编码字节喂入 parser, 再断言解析结果与预期一致。
//!
//! # 编码规则
//!
//! | 块类型   | 编码方式 |
//! |----------|----------|
//! | Header   | 写入 `hex` 的原始字节 |
//! | Length   | 从 `field_values` 取端口名对应值, 按 `field_type` 编码为整数写入 |
//! | Id       | 从 `field_values` 取端口名对应值, 按 `field_type` 编码为整数写入 |
//! | Field    | 从 `field_values` 取 `port_name` 对应值, 按 `field_type` 编码写入 |
//! | Field(Bytes) | 写入 `length_ref` 引用长度个字节, 首字节 = 端口值 |
//! | Bitfield | 在 `byte_offset`/`bit_offset` 位置写入 `bit_length` 位 (MSB first) |
//! | Checksum | 对覆盖范围字节计算校验值, 写入指定位置 |
//! | Tail     | 写入 `hex` 的原始字节 |
//!
//! # 示例
//!
//! ```ignore
//! use vofa_next_nodes::frame_decoder::FrameDecoderTestData;
//! use vofa_next_nodes::{DecoderBlockDef, FieldType};
//! use std::collections::HashMap;
//!
//! let blocks = vec![
//!     DecoderBlockDef::Header { id: "h1".into(), hex: "AA".into(), match_id: None },
//!     DecoderBlockDef::Field {
//!         id: "f1".into(), field_type: FieldType::UInt16LE,
//!         port_name: "ch0".into(), length_ref: None, match_id: None,
//!     },
//!     DecoderBlockDef::Tail { id: "t1".into(), hex: "BB".into(), match_id: None },
//! ];
//!
//! let mut values = HashMap::new();
//! values.insert("ch0".to_string(), 258.0); // 0x0102
//!
//! let bytes = FrameDecoderTestData::encode_frame(&blocks, &values);
//! assert_eq!(bytes, vec![0xAA, 0x02, 0x01, 0xBB]);
//! ```

use std::collections::HashMap;

use super::blocks::{block_should_execute, checksum_byte_len, parse_hex};
use super::ChecksumAlgorithm;
use schema_types::{
    DecoderBlockDef, DecoderChecksumCover, DecoderChecksumPosition, FieldType,
};

/// 帧解码器测试数据生成器 — 见模块文档
pub struct FrameDecoderTestData;

impl FrameDecoderTestData {
    /// 根据块定义和字段值编码一帧字节序列
    ///
    /// - `blocks`: 帧解码块定义列表 (与 [`FrameParser::new`] 参数一致)
    /// - `field_values`: 端口名 → 浮点值的映射
    ///
    /// 对于 Checksum 块, 自动计算校验值并写入。
    /// 对于 Length 块, 自动将值注册为 `length_values`, 供 Field(Bytes) 的 `length_ref` 引用。
    ///
    /// 返回编码后的完整帧字节序列。
    ///
    /// [`FrameParser::new`]: super::FrameParser::new
    pub fn encode_frame(
        blocks: &[DecoderBlockDef],
        field_values: &HashMap<String, f32>,
    ) -> Vec<u8> {
        Self::encode_frame_inner(blocks, field_values).0
    }

    /// 编码一帧, 返回 (字节流, 各 Checksum 块的字节位置 (buf_pos, cs_len))
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_possible_wrap, clippy::items_after_statements)]
    fn encode_frame_inner(
        blocks: &[DecoderBlockDef],
        field_values: &HashMap<String, f32>,
    ) -> (Vec<u8>, Vec<(usize, usize)>) {
        use DecoderChecksumCover::AllPrior;
        use DecoderChecksumPosition::{Append, Inline, Prepend};

        // 第一遍: 写入除 Checksum 外的所有字节
        let mut buf: Vec<u8> = Vec::new();
        // length_values: block_id → 长度值 (Bytes 类型的 Field 使用)
        let mut length_values: HashMap<String, u64> = HashMap::new();
        // 记录 checksum 块的信息, 第二遍写入校验值
        struct CsRecord {
            buf_pos: usize, // 当前 buf 长度 (插入位置)
            algorithm: ChecksumAlgorithm,
            custom_script: Option<String>,
            cover_begin: usize, // 校验覆盖起始 (在最终 buf 中的索引)
            #[allow(dead_code)]
            cover_end: usize, // 校验覆盖结束 (exclusive)
            position: DecoderChecksumPosition,
            cs_len: usize, // 校验值字节长度
        }
        let mut checksums: Vec<CsRecord> = Vec::new();
        // 记录 frame_start = Header 末尾在 buf 中的位置
        let mut frame_start: usize = 0;
        // 当前帧的 Id 值 (match_id 条件块的判定上下文, 与解析侧一致)
        let mut id_value: Option<i64> = None;

        for block in blocks {
            match block {
                DecoderBlockDef::Header { hex, .. } => {
                    frame_start = buf.len() + parse_hex(hex).len();
                    buf.extend_from_slice(&parse_hex(hex));
                }
                DecoderBlockDef::Length {
                    field_type,
                    port_name,
                    id,
                    match_id,
                    ..
                } => {
                    // match_id 不匹配时跳过 (不写入字节, 与解析侧一致)
                    if !block_should_execute(*match_id, id_value) {
                        continue;
                    }
                    let name = port_name.as_deref().unwrap_or("length").to_string();
                    let val = field_values.get(&name).copied().unwrap_or(0.0) as u64;
                    length_values.insert(id.clone(), val);
                    encode_int(&mut buf, *field_type, val);
                }
                DecoderBlockDef::Id {
                    field_type,
                    port_name,
                    ..
                } => {
                    let name = port_name.as_deref().unwrap_or("id_value").to_string();
                    let val = field_values.get(&name).copied().unwrap_or(0.0) as u64;
                    id_value = Some(val as i64);
                    encode_int(&mut buf, *field_type, val);
                }
                DecoderBlockDef::Field {
                    field_type,
                    port_name,
                    length_ref,
                    match_id,
                    ..
                } => {
                    // match_id 不匹配时跳过 (不写入字节, 与解析侧一致)
                    if !block_should_execute(*match_id, id_value) {
                        continue;
                    }
                    let val = field_values.get(port_name).copied().unwrap_or(0.0);
                    if *field_type == FieldType::Bytes {
                        // Bytes 类型: 写入 length_ref 指定的字节数
                        let len = length_ref
                            .as_deref()
                            .and_then(|ref_id| length_values.get(ref_id).copied())
                            .unwrap_or(1) as usize;
                        for j in 0..len {
                            buf.push((val as u8).wrapping_add(j as u8));
                        }
                    } else {
                        encode_float(&mut buf, *field_type, val);
                    }
                }
                DecoderBlockDef::Bitfield {
                    byte_offset,
                    bit_offset,
                    bit_length,
                    port_name,
                    match_id,
                    ..
                } => {
                    // match_id 不匹配时跳过 (不写入字节, 与解析侧一致)
                    if !block_should_execute(*match_id, id_value) {
                        continue;
                    }
                    let val = field_values.get(port_name).copied().unwrap_or(0.0) as u32;
                    let abs_byte_offset = frame_start + *byte_offset as usize;
                    let needed =
                        abs_byte_offset + (*bit_offset as usize + *bit_length as usize).div_ceil(8);
                    while buf.len() < needed {
                        buf.push(0);
                    }
                    // MSB first: 从 bit_offset 开始写入 bit_length 位
                    for i in 0..*bit_length {
                        let abs_bit = *bit_offset as usize + i as usize;
                        let byte_idx = abs_bit / 8;
                        let bit_in_byte = 7 - (abs_bit % 8);
                        let bit = (val >> (*bit_length - 1 - i)) & 1;
                        let idx = abs_byte_offset + byte_idx;
                        let mask = !(1u8 << bit_in_byte);
                        buf[idx] = (buf[idx] & mask) | ((bit as u8) << bit_in_byte);
                    }
                }
                DecoderBlockDef::Checksum {
                    algorithm,
                    custom_script,
                    cover,
                    cover_start,
                    cover_end: _,
                    position,
                    match_id,
                    ..
                } => {
                    // match_id 不匹配时跳过 (不写入字节, 与解析侧一致)
                    if !block_should_execute(*match_id, id_value) {
                        continue;
                    }
                    // 先放置占位字节 (全 0), 第二遍计算后替换
                    let cs_len = checksum_byte_len(*algorithm);
                    let placeholder = vec![0u8; cs_len];
                    let record = match position {
                        Prepend => {
                            // 校验字节在 frame_start 位置 — 先记下位置, 第二遍拼接处理
                            CsRecord {
                                buf_pos: frame_start,
                                algorithm: *algorithm,
                                custom_script: custom_script.clone(),
                                cover_begin: frame_start + cs_len, // 覆盖从占位之后开始
                                cover_end: 0,                      // 在第二遍确定
                                position: *position,
                                cs_len,
                            }
                        }
                        Inline | Append => {
                            let pos = buf.len();
                            buf.extend_from_slice(&placeholder);
                            let cover_begin = match cover {
                                AllPrior => frame_start,
                                DecoderChecksumCover::Range => {
                                    frame_start + (*cover_start).unwrap_or(0) as usize
                                }
                            };
                            CsRecord {
                                buf_pos: pos,
                                algorithm: *algorithm,
                                custom_script: custom_script.clone(),
                                cover_begin,
                                cover_end: pos, // 覆盖到 checksum 之前 (不含占位)
                                position: *position,
                                cs_len,
                            }
                        }
                    };
                    checksums.push(record);
                }
                DecoderBlockDef::Tail { hex, match_id, .. } => {
                    // match_id 不匹配时跳过 (不写入字节, 与解析侧一致)
                    if !block_should_execute(*match_id, id_value) {
                        continue;
                    }
                    buf.extend_from_slice(&parse_hex(hex));
                }
                // schema 扩展块由协议引擎 SchemaEngine 消费, FrameDecoder 测试数据生成跳过
                DecoderBlockDef::Csv { .. }
                | DecoderBlockDef::AsciiField { .. }
                | DecoderBlockDef::Samples { .. } => {}
            }
        }

        // 第二遍: 计算并写入校验值
        for cs in &checksums {
            // 对于 Inline/Append, cover_begin..cover_end 是校验覆盖范围
            // 对于 Prepend, cover_begin = frame_start + cs_len, cover_end = buf.len()
            let actual_cover_end = match cs.position {
                Prepend => buf.len(),
                _ => cs.cover_end,
            };
            let cover_bytes = &buf[cs.cover_begin..actual_cover_end];
            let computed = cs
                .algorithm
                .compute(cover_bytes, cs.custom_script.as_deref());
            // 将 computed 写入 buf[cs.buf_pos..buf_pos+cs_len]
            let write_len = computed.len().min(cs.cs_len);
            for j in 0..write_len {
                buf[cs.buf_pos + j] = computed[j];
            }
        }

        let cs_ranges = checksums.iter().map(|c| (c.buf_pos, c.cs_len)).collect();
        (buf, cs_ranges)
    }

    /// 编码多帧字节序列 (拼接 `encode_frame` 结果)
    ///
    /// - `blocks`: 共享的帧解码块定义
    /// - `frames`: 每帧的端口值映射列表
    ///
    /// 每帧的时间戳依次递增 1000 微秒。
    /// 返回连续拼接的完整字节流, 可直接喂入 [`FrameParser::feed`]。
    ///
    /// [`FrameParser::feed`]: super::FrameParser::feed
    pub fn encode_frames(blocks: &[DecoderBlockDef], frames: &[HashMap<String, f32>]) -> Vec<u8> {
        let mut all_bytes = Vec::new();
        for field_values in frames {
            let data = Self::encode_frame(blocks, field_values);
            all_bytes.extend_from_slice(&data);
        }
        all_bytes
    }

    /// 编码一帧, 强制设置 Id 块的值为 `id_val`
    ///
    /// 便捷方法: 设置端口名为 "id_value" 的字段值。
    #[allow(clippy::cast_precision_loss)]
    pub fn encode_frame_with_id(
        blocks: &[DecoderBlockDef],
        id_val: i64,
        field_values: &HashMap<String, f32>,
    ) -> Vec<u8> {
        let mut values = field_values.clone();
        values.insert("id_value".to_string(), id_val as f32);
        Self::encode_frame(blocks, &values)
    }

    /// 编码一帧但校验值错误 (用于测试校验失败场景)
    ///
    /// 在 `encode_frame` 的基础上, 翻转最后一个 Checksum 块的首字节,
    /// 使校验失败 (valid=false) 但帧结构 (含 Tail) 保持完整, 仍可正常解析。
    pub fn encode_frame_bad_checksum(
        blocks: &[DecoderBlockDef],
        field_values: &HashMap<String, f32>,
    ) -> Vec<u8> {
        let (mut data, cs_ranges) = Self::encode_frame_inner(blocks, field_values);
        // 翻转最后一个 Checksum 块的首字节, 使校验失败但帧结构 (含 Tail) 保持完整;
        // 无 Checksum 块时退化为翻转尾字节
        match cs_ranges.last() {
            Some(&(pos, len)) if len > 0 && pos < data.len() => data[pos] = !data[pos],
            _ => {
                if let Some(last) = data.last_mut() {
                    *last = !*last;
                }
            }
        }
        data
    }
}

/// 按 field_type 将 u64 整数编码为字节, 追加到 buf
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn encode_int(buf: &mut Vec<u8>, ft: FieldType, val: u64) {
    match ft {
        FieldType::UInt8 | FieldType::Int8 => {
            buf.push(val as u8);
        }
        FieldType::UInt16LE | FieldType::Int16LE => {
            buf.extend_from_slice(&(val as u16).to_le_bytes());
        }
        FieldType::UInt16BE | FieldType::Int16BE => {
            buf.extend_from_slice(&(val as u16).to_be_bytes());
        }
        FieldType::UInt32LE | FieldType::Int32LE => {
            buf.extend_from_slice(&(val as u32).to_le_bytes());
        }
        FieldType::UInt32BE | FieldType::Int32BE => {
            buf.extend_from_slice(&(val as u32).to_be_bytes());
        }
        FieldType::Float32LE | FieldType::Float32BE | FieldType::Bytes => {
            // Float/Bytes 不适合整数编码: 写入 0 占位
            buf.push(val as u8);
        }
    }
}

/// 按 field_type 将 f32 值编码为字节, 追加到 buf
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn encode_float(buf: &mut Vec<u8>, ft: FieldType, val: f32) {
    match ft {
        FieldType::UInt8 | FieldType::Int8 => {
            buf.push(val as u8);
        }
        FieldType::UInt16LE | FieldType::Int16LE => {
            buf.extend_from_slice(&(val as u16).to_le_bytes());
        }
        FieldType::UInt16BE | FieldType::Int16BE => {
            buf.extend_from_slice(&(val as u16).to_be_bytes());
        }
        FieldType::UInt32LE | FieldType::Int32LE => {
            buf.extend_from_slice(&(val as u32).to_le_bytes());
        }
        FieldType::UInt32BE | FieldType::Int32BE => {
            buf.extend_from_slice(&(val as u32).to_be_bytes());
        }
        FieldType::Float32LE => {
            buf.extend_from_slice(&val.to_le_bytes());
        }
        FieldType::Float32BE => {
            buf.extend_from_slice(&val.to_be_bytes());
        }
        FieldType::Bytes => {
            // Bytes 类型: 写入首字节
            buf.push(val as u8);
        }
    }
}
