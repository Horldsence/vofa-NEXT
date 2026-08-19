//! 帧解码块求值 — 各块类型的解析逻辑 (FrameParser::try_parse_frame_from)
//!
//! - Length/Id/Field: 按 field_type 读取 N 字节, 解码为 f32, 存到 outputs[port_name]
//! - Bitfield: 从 frame_start + byte_offset 读取字节, 按位解码 (不消耗 cursor)
//! - Checksum: 计算 expected vs actual, 设置 valid 标志 (position 决定字节位置)
//! - Tail: 匹配固定字节序列 (消耗字节)
//!
//! 多帧分派: match_id == None 始终输出; Some(v) 仅当 v == id_value 时输出
//! (但所有块都消耗字节)。
//! 变长字段: Length 块输出 length_value; Field 块的 length_ref 引用之, 决定 Bytes 类型长度。

use std::collections::HashMap;

use super::{ChecksumAlgorithm, FrameParser, ParseResult};
use crate::decoder_block::{
    DecoderBlockDef, DecoderChecksumCover, DecoderChecksumPosition, FieldType,
};

impl FrameParser {
    /// 从给定字节切片解析一帧 (核心解析逻辑)
    ///
    /// - `data`: 完整字节切片
    /// - `start`: 帧起始 (Header 开头) 在 data 中的索引
    /// - `frame_start`: Header 末尾 (= 字段起始) 在 data 中的索引
    pub(super) fn try_parse_frame_from(
        &self,
        data: &[u8],
        start: usize,
        frame_start: usize,
        timestamp_us: u64,
    ) -> Option<ParseResult> {
        let mut outputs: HashMap<String, f32> = HashMap::new();
        let mut valid = true; // 默认通过, Checksum 块可设置为 false
        let mut id_value: Option<i64> = None;
        let mut length_values: HashMap<String, u64> = HashMap::new(); // block_id → length

        // cursor: 当前读取位置 (相对 data 起点), 从 frame_start 开始
        let mut cursor = frame_start;

        for block in &self.blocks {
            match block {
                DecoderBlockDef::Header { .. } => {
                    // Header 已匹配, 跳过
                    continue;
                }
                DecoderBlockDef::Length {
                    field_type,
                    port_name,
                    match_id,
                    ..
                } => {
                    // 检查 match_id — 不匹配时跳过 (不消耗字节, 多帧分派布局条件性)
                    if !block_should_execute(*match_id, id_value) {
                        continue;
                    }

                    let n = field_type.byte_len()?;
                    if cursor + n > data.len() {
                        return None;
                    }
                    let bytes = &data[cursor..cursor + n];
                    let val = field_type.decode(bytes)?;
                    cursor += n;

                    // 记录 length_value (作为 u64), key = block.id (供 Field 的 length_ref 引用)
                    let len_val = val as u64;
                    length_values.insert(block.id().to_string(), len_val);

                    // 输出到 port_name (默认 "length")
                    let pname = port_name.clone().unwrap_or_else(|| "length".to_string());
                    outputs.insert(pname, val);
                }
                DecoderBlockDef::Id {
                    field_type,
                    port_name,
                    ..
                } => {
                    let n = field_type.byte_len()?;
                    if cursor + n > data.len() {
                        return None;
                    }
                    let bytes = &data[cursor..cursor + n];
                    let val = field_type.decode(bytes)?;
                    cursor += n;

                    // 设置 id_value 上下文 (i64)
                    id_value = Some(val as i64);

                    // 输出到 port_name (默认 "id_value")
                    let pname = port_name.clone().unwrap_or_else(|| "id_value".to_string());
                    outputs.insert(pname, val);
                }
                DecoderBlockDef::Field {
                    field_type,
                    port_name,
                    length_ref,
                    match_id,
                    ..
                } => {
                    // 检查 match_id — 不匹配时跳过 (不消耗字节, 多帧分派布局条件性)
                    if !block_should_execute(*match_id, id_value) {
                        continue;
                    }

                    // 确定读取字节数
                    let n = if *field_type == FieldType::Bytes {
                        // Bytes 类型: 用 length_ref 引用的 length_value
                        if let Some(ref_id) = length_ref {
                            length_values.get(ref_id).map(|&v| v as usize)
                        } else {
                            // 无 length_ref, 默认 0 字节
                            Some(0)
                        }
                    } else {
                        field_type.byte_len()
                    };

                    let n = match n {
                        Some(n) => n,
                        None => continue, // 无法确定长度, 跳过
                    };

                    if cursor + n > data.len() {
                        return None;
                    }

                    let bytes = &data[cursor..cursor + n];
                    cursor += n;

                    let val = field_type.decode(bytes).unwrap_or(0.0);
                    outputs.insert(port_name.clone(), val);
                }
                DecoderBlockDef::Bitfield {
                    byte_offset,
                    bit_offset,
                    bit_length,
                    is_signed,
                    port_name,
                    match_id,
                    ..
                } => {
                    // Bitfield 不消耗 cursor, 读取相对 frame_start 的字节
                    if !block_should_execute(*match_id, id_value) {
                        continue;
                    }

                    let abs_byte_offset = frame_start + *byte_offset as usize;
                    let total_bits = *bit_length as usize;
                    let needed_bytes = (total_bits + *bit_offset as usize).div_ceil(8);
                    if abs_byte_offset + needed_bytes > data.len() {
                        return None;
                    }

                    let val = read_bitfield(
                        &data[abs_byte_offset..abs_byte_offset + needed_bytes],
                        *bit_offset,
                        *bit_length,
                        *is_signed,
                    );
                    outputs.insert(port_name.clone(), val);
                }
                DecoderBlockDef::Checksum {
                    algorithm,
                    custom_script,
                    cover,
                    cover_start,
                    cover_end,
                    position,
                    match_id,
                    ..
                } => {
                    if !block_should_execute(*match_id, id_value) {
                        continue;
                    }

                    // 1. 计算校验覆盖范围
                    let (cover_begin, cover_end_idx) = match cover {
                        DecoderChecksumCover::AllPrior => {
                            // 从 header 之后到当前 cursor
                            (frame_start, cursor)
                        }
                        DecoderChecksumCover::Range => {
                            let cs = cover_start.unwrap_or(0) as usize;
                            let ce = cover_end.unwrap_or(0) as usize;
                            (frame_start + cs, frame_start + ce)
                        }
                    };

                    if cover_end_idx > data.len() || cover_begin > cover_end_idx {
                        return None;
                    }
                    let cover_bytes = &data[cover_begin..cover_end_idx];

                    // 2. 根据 position 读取校验字节
                    let cs_len = checksum_byte_len(*algorithm);
                    let cs_bytes = match position {
                        DecoderChecksumPosition::Inline => {
                            // 校验字节在当前 cursor 位置
                            if cursor + cs_len > data.len() {
                                return None;
                            }
                            let b = &data[cursor..cursor + cs_len];
                            cursor += cs_len;
                            b.to_vec()
                        }
                        DecoderChecksumPosition::Append => {
                            // 校验字节在帧末尾 (Tail 之前) — 此处简化为从 cursor 读取
                            if cursor + cs_len > data.len() {
                                return None;
                            }
                            let b = &data[cursor..cursor + cs_len];
                            cursor += cs_len;
                            b.to_vec()
                        }
                        DecoderChecksumPosition::Prepend => {
                            // 校验字节在 header 之后 (字段起始) — 不消耗 cursor
                            if frame_start + cs_len > data.len() {
                                return None;
                            }
                            data[frame_start..frame_start + cs_len].to_vec()
                        }
                    };

                    // 3. 验证
                    let script_ref = custom_script.as_deref();
                    if !algorithm.verify(cover_bytes, &cs_bytes, script_ref) {
                        valid = false;
                    }
                }
                DecoderBlockDef::Tail { hex, match_id, .. } => {
                    // 检查 match_id — 不匹配时跳过
                    if !block_should_execute(*match_id, id_value) {
                        continue;
                    }

                    let tail_bytes = parse_hex(hex);
                    if cursor + tail_bytes.len() > data.len() {
                        return None;
                    }
                    let actual = &data[cursor..cursor + tail_bytes.len()];
                    if actual != tail_bytes.as_slice() {
                        // Tail 不匹配 — 视为帧边界错误
                        // 返回 None 让调用方继续等待/重新查找 header
                        return None;
                    }
                    cursor += tail_bytes.len();
                }
                // schema 扩展块 (Csv/AsciiField/Samples) 由协议引擎 SchemaEngine 消费;
                // FrameDecoder 的 FrameParser 暂不支持, 跳过 (不消耗字节)
                DecoderBlockDef::Csv { .. }
                | DecoderBlockDef::AsciiField { .. }
                | DecoderBlockDef::Samples { .. } => {
                    continue;
                }
            }
        }

        // 计算消耗字节数 (从 start 到 cursor)
        let consumed_bytes = cursor - start;

        Some(ParseResult {
            frame: super::ParsedFrame {
                outputs,
                valid,
                timestamp_us,
                id_value,
                raw_bytes: Vec::new(),
            },
            consumed_bytes,
        })
    }
}

// ============ 工具函数 ============

/// 判断块是否应执行 (基于 match_id 与 id_value)
pub(super) fn block_should_execute(match_id: Option<i64>, id_value: Option<i64>) -> bool {
    match match_id {
        None => true,                   // 无 match_id → 始终执行
        Some(v) => id_value == Some(v), // 有 match_id → 仅当 id_value 匹配时执行
    }
}

/// 在 buf 中查找 subsequence
pub(super) fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// 读取位域值
///
/// - `bytes`: 起始字节切片 (至少包含 bit_offset + bit_length 位)
/// - `bit_offset`: 起始位偏移 (0-7, MSB first)
/// - `bit_length`: 位长度 (1-32)
/// - `is_signed`: 是否带符号 (true=最高位为符号位, 二补码)
fn read_bitfield(bytes: &[u8], bit_offset: u8, bit_length: u8, is_signed: bool) -> f32 {
    if bit_length == 0 || bytes.is_empty() {
        return 0.0;
    }

    // 按位读取, MSB first
    let mut value: u32 = 0;
    for i in 0..bit_length as usize {
        let abs_bit = bit_offset as usize + i;
        let byte_idx = abs_bit / 8;
        let bit_in_byte = 7 - (abs_bit % 8); // MSB first: bit 7 是最高位
        if byte_idx >= bytes.len() {
            break;
        }
        let bit = (bytes[byte_idx] >> bit_in_byte) & 1;
        value = (value << 1) | bit as u32;
    }

    // 符号扩展
    if is_signed && bit_length < 32 {
        let sign_bit = 1u32 << (bit_length - 1);
        if value & sign_bit != 0 {
            // 负数: 二补码扩展
            let mask = u32::MAX << bit_length;
            value |= mask;
        }
    }

    if is_signed {
        (value as i32) as f32
    } else {
        value as f32
    }
}

/// 获取校验算法输出的字节长度 (实现已迁移至 core::schema)
pub(super) fn checksum_byte_len(algo: ChecksumAlgorithm) -> usize {
    algo.byte_len()
}

// ============ HEX 解析工具 ============

/// 解析 HEX 字符串为字节切片 (实现已迁移至 core::schema, 供两 crate 共用)
///
/// 输入格式: "AA BB" / "AABB" / "aa bb" / "0xAA 0xBB" 均可,
/// 空格/逗号/0x 前缀均会被忽略。
///
/// 解析失败 (奇数长度 / 非法字符) 返回空 Vec。
pub fn parse_hex(hex: &str) -> Vec<u8> {
    vofa_next_core::parse_hex(hex)
}
