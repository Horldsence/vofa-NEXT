//! SchemaEngine — 自定义帧 schema 的流式协议引擎
//!
//! 协议 = 一份帧 schema (块列表): 解析是帧解码 (decode 块), 发送是帧编码
//! (encode 块), 共用同一份定义。本引擎处理 `SchemaPreset::Custom` 的
//! schema; 预设 schema 由 [`crate::compile_schema`] 分发到 legacy 引擎。
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

use std::collections::HashMap;

use vofa_next_core::{
    AsciiBase, DataFrame, DecoderBlockDef, DecoderChecksumCover, DecoderChecksumPosition,
    FieldType, ProtocolSchema, SchemaPreset,
};

use crate::engine::{FeedOutput, ProtocolEngine};
use crate::logic_decoder::LogicDecoderEngine;

/// 自定义 schema 协议引擎
pub struct SchemaEngine {
    /// 帧 schema (preset 必为 Custom)
    schema: ProtocolSchema,
    /// 派生端口列表 (输出 DataFrame.channels 的顺序)
    ports: Vec<String>,
    /// 流式字节缓冲
    buf: Vec<u8>,
    /// Samples 块委托的逻辑解码引擎 (仅 decode 含 Samples 块时存在)
    logic: Option<Box<LogicDecoderEngine>>,
}

/// 单帧解析尝试结果
enum ParseAttempt {
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
    fn header_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for b in &self.schema.decode {
            if let DecoderBlockDef::Header { hex, .. } = b {
                bytes.extend_from_slice(&vofa_next_core::parse_hex(hex));
            }
        }
        bytes
    }

    /// 从 data[0..] 尝试解析一帧 (data 起点 = header 起点或帧起点)
    ///
    /// `frame_start`: header 末尾 (= 字段起始) 在 data 中的索引
    fn try_parse(&self, data: &[u8], frame_start: usize) -> ParseAttempt {
        let mut outputs: HashMap<String, f32> = HashMap::new();
        let mut valid = true;
        let mut id_value: Option<i64> = None;
        let mut length_values: HashMap<String, u64> = HashMap::new();
        let mut cursor = frame_start;

        for block in &self.schema.decode {
            // 多帧分派: match_id 不匹配时跳过 (不消耗字节)
            let match_id = block.match_id();
            if match_id.is_some() && match_id != id_value {
                continue;
            }
            match block {
                DecoderBlockDef::Header { .. } => continue, // 已匹配
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
                    let val = field_type
                        .decode(&data[cursor..cursor + n])
                        .unwrap_or(0.0);
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
                    let val =
                        read_bitfield(&data[abs..abs + needed], *bit_offset, *bit_length, *is_signed);
                    outputs.insert(port_name.clone(), val);
                }
                DecoderBlockDef::Csv { separator, ports } => {
                    // 一行 = 一帧: 找行尾 '\n', 按分隔符切分列
                    // (ASCII 文本帧, lossy 转换安全; 单/多字节分隔符统一走 str::split)
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
                        DecoderChecksumPosition::Inline
                        | DecoderChecksumPosition::Append => {
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
                    let tail = vofa_next_core::parse_hex(hex);
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
                    continue;
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

impl ProtocolEngine for SchemaEngine {
    fn feed(&mut self, data: &[u8]) -> FeedOutput {
        // Samples 块: 整体委托逻辑解码引擎
        if let Some(logic) = &mut self.logic {
            return logic.feed(data);
        }

        self.buf.extend_from_slice(data);
        let header = self.header_bytes();
        let mut frames = Vec::new();
        // 批内所有帧共享一个时间戳 (每次 feed 只读一次时钟)
        let ts = vofa_next_core::now_us();
        let mut base = 0usize;

        loop {
            // 1. 帧定界: 定位 Header (无 Header 块则从当前位置起解析)
            let frame_start = if header.is_empty() {
                0
            } else {
                match self.buf[base..]
                    .windows(header.len())
                    .position(|w| w == header.as_slice())
                {
                    Some(pos) => {
                        base += pos;
                        header.len()
                    }
                    None => {
                        // 未找到 header: 保留末尾 header.len()-1 字节 (跨包截断)
                        let keep = header.len().saturating_sub(1);
                        base = base.max(self.buf.len().saturating_sub(keep));
                        break;
                    }
                }
            };

            // 2. 按 decode 块求值
            match self.try_parse(&self.buf[base..], frame_start) {
                ParseAttempt::Incomplete => break,
                ParseAttempt::Invalid => {
                    // 假同步: 丢弃到本帧头之后, 重新同步
                    base += frame_start.max(1);
                }
                ParseAttempt::Done {
                    outputs,
                    valid,
                    consumed,
                } => {
                    base += consumed;
                    if valid {
                        // checksum 失败跳过该帧; 输出按端口序
                        let channels = self
                            .ports
                            .iter()
                            .map(|p| outputs.get(p).copied().unwrap_or(0.0))
                            .collect();
                        frames.push(DataFrame::with_timestamp(ts, channels));
                    }
                }
            }
        }

        if base > 0 {
            self.buf.drain(..base);
        }
        // 防止缓冲区无限增长 (与 JustFloatEngine 一致)
        if self.buf.len() > 8192 {
            let drop = self.buf.len() - 4096;
            self.buf.drain(..drop);
        }

        FeedOutput::from_frames(frames)
    }

    fn encode_channel(&mut self, channel: usize, value: f32) -> Vec<u8> {
        // 单通道发送: 该通道写值, 其余通道 0 (与 legacy 引擎语义一致)
        let mut values = vec![0.0f32; self.ports.len().max(channel + 1)];
        if channel < values.len() {
            values[channel] = value;
        }
        self.encode_channels(&values)
    }

    fn encode_channels(&mut self, values: &[f32]) -> Vec<u8> {
        match &self.schema.encode {
            Some(blocks) => vofa_next_core::encode_by_blocks(blocks, &self.ports, values),
            // Custom schema 未定义 encode 块: 无编码约定, 返回空
            None => Vec::new(),
        }
    }

    fn name(&self) -> &str {
        "CustomSchema"
    }

    fn take_pending(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.buf)
    }

    fn new_worker(&self) -> Box<dyn ProtocolEngine> {
        Box::new(SchemaEngine::new(self.schema.clone()))
    }
}

/// 读取位域值 (与 FrameParser 的 read_bitfield 语义一致)
///
/// - `bytes`: 起始字节切片 (至少包含 bit_offset + bit_length 位)
/// - `bit_offset`: 起始位偏移 (0-7, MSB first)
/// - `bit_length`: 位长度 (1-32)
/// - `is_signed`: 是否带符号 (true=最高位为符号位, 二补码)
fn read_bitfield(bytes: &[u8], bit_offset: u8, bit_length: u8, is_signed: bool) -> f32 {
    if bit_length == 0 || bytes.is_empty() {
        return 0.0;
    }
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
    if is_signed && bit_length < 32 {
        let sign_bit = 1u32 << (bit_length - 1);
        if value & sign_bit != 0 {
            value |= u32::MAX << bit_length;
        }
    }
    if is_signed {
        (value as i32) as f32
    } else {
        value as f32
    }
}

/// 预设对应的缺省 legacy 配置 (legacy_config 缺失时的兜底)
fn default_legacy_config(preset: SchemaPreset) -> Option<vofa_next_core::ProtocolConfig> {
    use vofa_next_core::ProtocolConfig;
    match preset {
        SchemaPreset::JustFloat => Some(ProtocolConfig::JustFloat { channels: None }),
        SchemaPreset::FireWater => Some(ProtocolConfig::FireWater { channels: None }),
        SchemaPreset::RawData => Some(ProtocolConfig::RawData),
        SchemaPreset::Slcan => Some(ProtocolConfig::Slcan),
        SchemaPreset::CandleLight => Some(ProtocolConfig::CandleLight),
        // LogicDecode 需要具体解码器配置, 无合理缺省
        SchemaPreset::LogicDecode | SchemaPreset::Custom => None,
    }
}

/// 编译帧 schema 为协议引擎
///
/// - `preset != Custom`: 用 `legacy_config` (缺失时按预设兜底) 走现有
///   [`crate::create_engine`], 完整保留自动检测 / 并行 split / CAN / 逻辑事件能力;
/// - `Custom`: 构造 [`SchemaEngine`] (流式帧解码 + encode 块编码)。
pub fn compile_schema(schema: &ProtocolSchema) -> Box<dyn ProtocolEngine> {
    if schema.preset != SchemaPreset::Custom {
        let config = schema
            .legacy_config
            .clone()
            .or_else(|| default_legacy_config(schema.preset));
        if let Some(config) = config {
            return crate::create_engine(&config);
        }
        // 无 legacy 配置可用 (如 LogicDecode 缺 decoder): 回落 SchemaEngine
    }
    Box::new(SchemaEngine::new(schema.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vofa_next_core::{
        ChecksumAlgorithm, DecoderChecksumCover, DecoderChecksumPosition, EncodeBlockDef,
        ProtocolConfig,
    };

    /// JustFloat 等价的自定义 schema (2×float32LE field + tail)
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
    fn test_custom_schema_justfloat_equivalent_decode() {
        let mut engine = compile_schema(&justfloat_like_schema());
        let mut data = Vec::new();
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&2.0f32.to_le_bytes());
        data.extend_from_slice(&[0x00, 0x00, 0x80, 0x7F]);

        let frames = engine.feed(&data).frames;
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].channels, vec![1.0, 2.0]);
    }

    #[test]
    fn test_custom_schema_partial_feed() {
        // 跨包截断: 分两次喂入应拼出完整帧 (2×float32LE + tail = 12 字节, 从第 3 字节处切开)
        let mut engine = compile_schema(&justfloat_like_schema());
        let mut data = Vec::new();
        data.extend_from_slice(&1.5f32.to_le_bytes());
        data.extend_from_slice(&2.5f32.to_le_bytes());
        data.extend_from_slice(&[0x00, 0x00, 0x80, 0x7F]);

        assert!(engine.feed(&data[..3]).frames.is_empty());
        let frames = engine.feed(&data[3..]).frames;
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].channels, vec![1.5, 2.5]);
    }

    #[test]
    fn test_custom_schema_encode_roundtrip() {
        // 编码 → 解码 往返 (JustFloat 等价布局)
        let mut engine = compile_schema(&justfloat_like_schema());
        let bytes = engine.encode_channels(&[3.0, 4.0]);
        let mut expect = Vec::new();
        expect.extend_from_slice(&3.0f32.to_le_bytes());
        expect.extend_from_slice(&4.0f32.to_le_bytes());
        expect.extend_from_slice(&[0x00, 0x00, 0x80, 0x7F]);
        assert_eq!(bytes, expect);

        let mut decoder = compile_schema(&justfloat_like_schema());
        let frames = decoder.feed(&bytes).frames;
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].channels, vec![3.0, 4.0]);
    }

    #[test]
    fn test_custom_schema_csv_decode() {
        let schema = ProtocolSchema {
            preset: SchemaPreset::Custom,
            legacy_config: None,
            decode: vec![DecoderBlockDef::Csv {
                separator: ",".into(),
                ports: vec!["x".into(), "y".into()],
            }],
            encode: None,
        };
        let mut engine = compile_schema(&schema);
        let frames = engine.feed(b"1.0,2.0\n").frames;
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].channels, vec![1.0, 2.0]);

        // 多行 + CRLF
        let frames = engine.feed(b"3.5,4.5\r\n5.0,6.0\n").frames;
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].channels, vec![3.5, 4.5]);
        assert_eq!(frames[1].channels, vec![5.0, 6.0]);
    }

    #[test]
    fn test_custom_schema_header_length_field_checksum() {
        // header + length(uint8) + bytes 字段(length_ref) + sum8 校验 + tail
        let schema = ProtocolSchema {
            preset: SchemaPreset::Custom,
            legacy_config: None,
            decode: vec![
                DecoderBlockDef::Header {
                    id: "h".into(),
                    hex: "AA".into(),
                    match_id: None,
                },
                DecoderBlockDef::Length {
                    id: "len".into(),
                    field_type: FieldType::UInt8,
                    port_name: None,
                    unit: None,
                    match_id: None,
                },
                DecoderBlockDef::Field {
                    id: "payload".into(),
                    field_type: FieldType::Bytes,
                    port_name: "p".into(),
                    length_ref: Some("len".into()),
                    match_id: None,
                },
                DecoderBlockDef::Checksum {
                    id: "cs".into(),
                    algorithm: ChecksumAlgorithm::Sum8,
                    custom_script: None,
                    cover: DecoderChecksumCover::AllPrior,
                    cover_start: None,
                    cover_end: None,
                    position: DecoderChecksumPosition::Inline,
                    match_id: None,
                },
                DecoderBlockDef::Tail {
                    id: "t".into(),
                    hex: "BB".into(),
                    match_id: None,
                },
            ],
            encode: None,
        };
        let mut engine = compile_schema(&schema);
        // AA 02 07 08 (sum8: 02+07+08=17=0x11) BB
        let good = [0xAA, 0x02, 0x07, 0x08, 0x11, 0xBB];
        let frames = engine.feed(&good).frames;
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].channels, vec![7.0]); // Bytes 输出第一字节

        // 校验失败: 帧被跳过
        let bad = [0xAA, 0x02, 0x07, 0x08, 0x12, 0xBB];
        assert!(engine.feed(&bad).frames.is_empty());
    }

    #[test]
    fn test_custom_schema_ascii_field() {
        // Slcan 类: header 'T' + 3 位 hex id + tail '\r'
        let schema = ProtocolSchema {
            preset: SchemaPreset::Custom,
            legacy_config: None,
            decode: vec![
                DecoderBlockDef::Header {
                    id: "h".into(),
                    hex: "54".into(), // 'T'
                    match_id: None,
                },
                DecoderBlockDef::AsciiField {
                    port_name: "id".into(),
                    base: AsciiBase::Hex,
                    digits: 3,
                },
                DecoderBlockDef::Tail {
                    id: "t".into(),
                    hex: "0D".into(), // '\r'
                    match_id: None,
                },
            ],
            encode: None,
        };
        let mut engine = compile_schema(&schema);
        let frames = engine.feed(b"T1A3\r").frames;
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].channels, vec![0x1A3 as f32]);
    }

    #[test]
    fn test_custom_schema_resync_after_garbage() {
        // 垃圾前缀 + 假 header 后能重新同步到真帧
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
                    id: "f".into(),
                    field_type: FieldType::UInt8,
                    port_name: "v".into(),
                    length_ref: None,
                    match_id: None,
                },
                DecoderBlockDef::Tail {
                    id: "t".into(),
                    hex: "BB".into(),
                    match_id: None,
                },
            ],
            encode: None,
        };
        let mut engine = compile_schema(&schema);
        // 垃圾 + 假 header (AA 后 tail 不匹配) + 真帧
        let data = [0x00, 0xAA, 0x11, 0x22, 0xAA, 0x2A, 0xBB];
        let frames = engine.feed(&data).frames;
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].channels, vec![42.0]);
    }

    #[test]
    fn test_compile_schema_preset_returns_legacy_engine() {
        // 预设路径: legacy_config 存在 → 对应 legacy 引擎
        let schema = ProtocolSchema {
            preset: SchemaPreset::JustFloat,
            legacy_config: Some(ProtocolConfig::JustFloat { channels: Some(2) }),
            decode: vec![],
            encode: None,
        };
        let engine = compile_schema(&schema);
        assert_eq!(engine.name(), "JustFloat");

        // legacy_config 缺失 → 按预设兜底 (FireWater 自动模式)
        let schema = ProtocolSchema {
            preset: SchemaPreset::FireWater,
            legacy_config: None,
            decode: vec![],
            encode: None,
        };
        let engine = compile_schema(&schema);
        assert_eq!(engine.name(), "FireWater");
        assert!(engine.is_auto_mode());

        let schema = ProtocolSchema {
            preset: SchemaPreset::Slcan,
            legacy_config: None,
            decode: vec![],
            encode: None,
        };
        assert_eq!(compile_schema(&schema).name(), "Slcan");

        let schema = ProtocolSchema {
            preset: SchemaPreset::CandleLight,
            legacy_config: None,
            decode: vec![],
            encode: None,
        };
        assert_eq!(compile_schema(&schema).name(), "CandleLight");

        let schema = ProtocolSchema {
            preset: SchemaPreset::RawData,
            legacy_config: None,
            decode: vec![],
            encode: None,
        };
        assert_eq!(compile_schema(&schema).name(), "RawData");
    }
}
