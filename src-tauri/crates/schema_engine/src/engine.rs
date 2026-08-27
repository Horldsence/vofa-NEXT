//! SchemaEngine 核心 — 帧定界 + 按块解析

use std::collections::HashMap;

use logic_decoder::LogicDecoderEngine;
use schema_types::{DecoderBlockDef, ProtocolSchema};

use crate::bitfield::read_bitfield;

/// 自定义 schema 协议引擎
pub struct SchemaEngine {
    /// 帧 schema (preset 必为 Custom)
    pub(crate) schema: ProtocolSchema,
    /// 派生端口列表 (输出 DataFrame.channels 的顺序)
    pub(crate) ports: Vec<String>,
    /// 流式字节缓冲
    pub(crate) buf: Vec<u8>,
    /// Samples 块委托的逻辑解码引擎 (仅 decode 含 Samples 块时存在)
    pub(crate) logic: Option<Box<LogicDecoderEngine>>,
}

/// 单帧解析尝试结果
pub(crate) enum ParseAttempt {
    /// 字节不足, 等待更多数据
    Incomplete,
    /// 帧结构错误 (Tail 不匹配 / ASCII 解析失败) — 调用方重新同步
    Invalid,
    /// 解析完成 (valid = checksum 是否通过)
    Done {
        outputs: HashMap<String, f32>,
        valid: bool,
        consumed: usize,
    },
}

impl SchemaEngine {
    pub fn new(schema: ProtocolSchema) -> Self {
        let ports = schema.port_names();
        // decode 含 Samples 块: 整体委托逻辑解码 (混合布局语义不明确, 不支持)
        let logic = schema.decode.iter().find_map(|b| match b {
            DecoderBlockDef::Samples { decoder } => {
                Some(Box::new(LogicDecoderEngine::new(decoder.clone())))
            }
            _ => None,
        });
        Self {
            schema,
            ports,
            buf: Vec::with_capacity(1024),
            logic,
        }
    }

    /// 收集所有 Header 块的字节 (按顺序拼接, 与 FrameParser 一致)
    pub(crate) fn header_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for b in &self.schema.decode {
            if let DecoderBlockDef::Header { hex, .. } = b {
                bytes.extend_from_slice(&schema_types::parse_hex(hex));
            }
        }
        bytes
    }

    /// 从 data[0..] 尝试解析一帧 (data 起点 = header 起点或帧起点)
    ///
    /// `frame_start`: header 末尾 (= 字段起始) 在 data 中的索引
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub(crate) fn try_parse(&self, data: &[u8], frame_start: usize) -> ParseAttempt {
        use schema_types::{AsciiBase, DecoderChecksumCover, DecoderChecksumPosition, FieldType};
        let mut outputs: HashMap<String, f32> = HashMap::new();
        let mut valid = true;
        let mut id_value: Option<i64> = None;
        let mut length_values: HashMap<String, u64> = HashMap::new();
        let mut cursor = frame_start;

        for block in &self.schema.decode {
            // 多帧分派: match_id 不匹配时跳过 (跳过)
            let match_id = block.match_id();
            if match_id.is_some() && match_id != id_value {
                continue;
            }
            match block {
                DecoderBlockDef::Header { .. } => {} // 已匹配
                DecoderBlockDef::Length {
                    id,
                    field_type,
                    port_name,
                    ..
                } => {
                    let Some(n) = field_type.byte_len() else {
                        return ParseAttempt::Invalid;
                    };
                    if cursor + n > data.len() {
                        return ParseAttempt::Incomplete;
                    }
                    let Some(val) = field_type.decode(&data[cursor..cursor + n]) else {
                        return ParseAttempt::Invalid;
                    };
                    cursor += n;
                    length_values.insert(id.clone(), val as u64);
                    let pname = port_name.clone().unwrap_or_else(|| "length".to_string());
                    outputs.insert(pname, val);
                }
                DecoderBlockDef::Id {
                    field_type,
                    port_name,
                    ..
                } => {
                    let Some(n) = field_type.byte_len() else {
                        return ParseAttempt::Invalid;
                    };
                    if cursor + n > data.len() {
                        return ParseAttempt::Incomplete;
                    }
                    let Some(val) = field_type.decode(&data[cursor..cursor + n]) else {
                        return ParseAttempt::Invalid;
                    };
                    cursor += n;
                    id_value = Some(val as i64);
                    let pname = port_name.clone().unwrap_or_else(|| "id_value".to_string());
                    outputs.insert(pname, val);
                }
                DecoderBlockDef::Field {
                    field_type,
                    port_name,
                    length_ref,
                    ..
                } => {
                    // 确定读取字节数 (Bytes 类型由 length_ref 引用 Length 块的值)
                    let n = if *field_type == FieldType::Bytes {
                        match length_ref {
                            Some(ref_id) => match length_values.get(ref_id) {
                                Some(&v) => v as usize,
                                None => continue, // 无法确定长度, 跳过
                            },
                            None => 0,
                        }
                    } else {
                        match field_type.byte_len() {
                            Some(n) => n,
                            None => continue,
                        }
                    };
                    if cursor + n > data.len() {
                        return ParseAttempt::Incomplete;
                    }
                    let val = field_type.decode(&data[cursor..cursor + n]).unwrap_or(0.0);
                    cursor += n;
                    outputs.insert(port_name.clone(), val);
                }
                DecoderBlockDef::Bitfield {
                    byte_offset,
                    bit_offset,
                    bit_length,
                    is_signed,
                    port_name,
                    ..
                } => {
                    // 不消耗 cursor, 读取相对 frame_start 的字节
                    let abs = frame_start + *byte_offset as usize;
                    let needed = (*bit_length as usize + *bit_offset as usize).div_ceil(8);
                    if abs + needed > data.len() {
                        return ParseAttempt::Incomplete;
                    }
                    let val = read_bitfield(
                        &data[abs..abs + needed],
                        *bit_offset,
                        *bit_length,
                        *is_signed,
                    );
                    outputs.insert(port_name.clone(), val);
                }
                DecoderBlockDef::Csv { separator, ports } => {
                    // 一行 = 一帧: 找行尾 '\n', 按分隔符切分列
                    // (ASCII 文本 帧, lossy 转换安全; 单/多字节分隔符统一走 str::split)
                    let Some(nl) = data[cursor..].iter().position(|&b| b == b'\n') else {
                        return ParseAttempt::Incomplete;
                    };
                    let line = &data[cursor..cursor + nl];
                    let line = line.strip_suffix(b"\r").unwrap_or(line);
                    let line = String::from_utf8_lossy(line);
                    for (i, port) in ports.iter().enumerate() {
                        if let Some(tok) = line.split(separator.as_str()).nth(i) {
                            let v = tok.trim().parse::<f32>().unwrap_or(0.0);
                            outputs.insert(port.clone(), v);
                        }
                    }
                    cursor += nl + 1;
                }
                DecoderBlockDef::AsciiField {
                    port_name,
                    base,
                    digits,
                } => {
                    if cursor + digits > data.len() {
                        return ParseAttempt::Incomplete;
                    }
                    let s = &data[cursor..cursor + digits];
                    let radix = match base {
                        AsciiBase::Hex => 16,
                        AsciiBase::Dec => 10,
                    };
                    let Ok(text) = std::str::from_utf8(s) else {
                        return ParseAttempt::Invalid;
                    };
                    let Ok(v) = u64::from_str_radix(text, radix) else {
                        return ParseAttempt::Invalid;
                    };
                    cursor += digits;
                    outputs.insert(port_name.clone(), v as f32);
                }
                DecoderBlockDef::Checksum {
                    algorithm,
                    custom_script,
                    cover,
                    cover_start,
                    cover_end,
                    position,
                    ..
                } => {
                    // 覆盖范围 (与 FrameParser 语义一致)
                    let (cover_begin, cover_end_idx) = match cover {
                        DecoderChecksumCover::AllPrior => (frame_start, cursor),
                        DecoderChecksumCover::Range => {
                            let cs = cover_start.unwrap_or(0) as usize;
                            let ce = cover_end.unwrap_or(0) as usize;
                            (frame_start + cs, frame_start + ce)
                        }
                    };
                    if cover_end_idx > data.len() {
                        return ParseAttempt::Incomplete;
                    }
                    if cover_begin > cover_end_idx {
                        return ParseAttempt::Invalid;
                    }
                    let cover_bytes = &data[cover_begin..cover_end_idx];

                    // 校验字节位置
                    let cs_len = algorithm.byte_len();
                    let cs_bytes = match position {
                        // Inline/Append 均从当前 cursor 读取 (Append 简化为 cursor 处)
                        DecoderChecksumPosition::Inline | DecoderChecksumPosition::Append => {
                            if cursor + cs_len > data.len() {
                                return ParseAttempt::Incomplete;
                            }
                            let b = data[cursor..cursor + cs_len].to_vec();
                            cursor += cs_len;
                            b
                        }
                        DecoderChecksumPosition::Prepend => {
                            if frame_start + cs_len > data.len() {
                                return ParseAttempt::Incomplete;
                            }
                            data[frame_start..frame_start + cs_len].to_vec()
                        }
                    };
                    if !algorithm.verify(cover_bytes, &cs_bytes, custom_script.as_deref()) {
                        valid = false;
                    }
                }
                DecoderBlockDef::Tail { hex, .. } => {
                    let tail = schema_types::parse_hex(hex);
                    if cursor + tail.len() > data.len() {
                        return ParseAttempt::Incomplete;
                    }
                    if data[cursor..cursor + tail.len()] != tail[..] {
                        // 帧边界错误 — 假同步, 重新查找 header
                        return ParseAttempt::Invalid;
                    }
                    cursor += tail.len();
                }
                DecoderBlockDef::Samples { .. } => {
                    // Samples 整体委托逻辑解码引擎 (见 new), 不参与二进制帧解析
                }
            }
        }

        ParseAttempt::Done {
            outputs,
            valid,
            consumed: cursor,
        }
    }
}
