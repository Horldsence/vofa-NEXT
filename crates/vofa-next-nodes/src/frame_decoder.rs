//! # 帧解码状态机 (FrameDecoder)
//!
//! 镜像前端 `DecoderBlock` 块列表, 后端实现字节流 → 帧解析 → 输出端口值。
//!
//! 跨帧持久化: 由 data_loop 通过 `decoder_states: HashMap<widget_id, FrameParser>` 管理,
//! 与 `filter_states` 模式一致 — 节点首次出现时创建, 配置变化时重建。
//!
//! 状态机阶段 (阶段2 实现完整逻辑):
//! 1. WAIT_FOR_HEADER: 累积字节, 匹配 Header.hex 后进入 PARSE_FIELDS
//! 2. PARSE_FIELDS: 按 blocks 顺序读取 Length/Id/Field/Bitfield/Checksum/Tail
//! 3. 任何阶段失败 → 回到 WAIT_FOR_HEADER, 丢弃已读字节, 重新匹配帧头
//!
//! 多帧分派: Id 块设置 id_value 上下文, 后续块的 match_id 字段决定是否执行
//! 变长字段: Length 块输出 length_value, Field 块的 length_ref 引用之, 决定 Bytes 类型长度

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::DecoderBlockDef;

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
            ChecksumAlgorithm::None => Vec::new(),
            ChecksumAlgorithm::Sum8 => {
                let s: u8 = data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
                vec![s]
            }
            ChecksumAlgorithm::Xor8 => {
                let x: u8 = data.iter().fold(0u8, |acc, &b| acc ^ b);
                vec![x]
            }
            ChecksumAlgorithm::Crc8 => vec![crc8(data, 0x07, 0x00, 0x00)],
            ChecksumAlgorithm::Crc16Modbus => {
                let crc = crc16_modbus(data);
                crc.to_le_bytes().to_vec()
            }
            ChecksumAlgorithm::Crc16CCITT => {
                let crc = crc16_ccitt(data);
                crc.to_be_bytes().to_vec()
            }
            ChecksumAlgorithm::Crc32 => {
                let crc = crc32(data);
                crc.to_le_bytes().to_vec()
            }
            ChecksumAlgorithm::Lrc => {
                let lrc: u8 = data.iter().fold(0u8, |acc, &b| acc.wrapping_sub(b));
                vec![lrc]
            }
            ChecksumAlgorithm::Custom => {
                // 自定义脚本暂不支持后端求值 (前端 lib/checksum.ts 中的 customChecksum 用 JS 实现)
                // 后端此处返回空 Vec, 实际项目应在阶段2 引入 rhai/boa 等 JS 引擎求值
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
        crc ^= b as u16;
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
        crc ^= (b as u16) << 8;
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
        crc ^= b as u32;
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

// ============ ParsedFrame ============

/// 单帧解析结果
#[derive(Debug, Clone, Default)]
pub struct ParsedFrame {
    /// port_name → value (来自 field/bitfield/length/id 块)
    pub outputs: HashMap<String, f32>,
    /// 校验是否通过 (false=未通过/未收到)
    pub valid: bool,
    /// 帧时间戳 (微秒)
    pub timestamp_us: u64,
    /// 当前 id_value (用于多帧分派, None=未设置)
    pub id_value: Option<i64>,
}

/// 内部解析结果 (含消耗字节数)
#[derive(Debug, Clone)]
struct ParseResult {
    frame: ParsedFrame,
    /// 本帧消耗的字节数 (包括 header + 所有 blocks 消耗的字节)
    consumed_bytes: usize,
}

// ============ FrameParser 状态机 ============

/// 解析器内部状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParseState {
    /// 等待帧头
    WaitForHeader,
    /// 解析字段 (已匹配 header, frame_start 已设置)
    ParseFields,
}

/// 帧解析状态机
///
/// 跨帧持久化: 由调用方 (data_loop) 通过 `decoder_states: HashMap<widget_id, FrameParser>` 管理。
/// 当 blocks 配置变化时, 调用方应重建 FrameParser (使用 `matches_config` 检测)。
///
/// 解析流程:
/// 1. WaitForHeader: 累积字节, 在 buf 中查找 Header.hex 字节序列
///    - 找到: 丢弃 header 之前的字节, 进入 ParseFields, frame_start = header.len()
///    - 未找到: 保留最后 header.len()-1 字节 (避免跨包截断), 等待更多数据
/// 2. ParseFields: 按 blocks 顺序解析 (跳过 Header)
///    - Length/Id/Field: 按 field_type 读取 N 字节, 解码为 f32, 存到 outputs[port_name]
///    - Bitfield: 从 frame_start + byte_offset 读取字节, 按位解码 (不消耗 cursor)
///    - Checksum: 计算 expected vs actual, 设置 valid 标志 (不消耗 cursor, position 决定字节位置)
///    - Tail: 匹配固定字节序列 (消耗字节)
/// 3. 解析完成: 丢弃 consumed_bytes, 回到 WaitForHeader
///
/// 多帧分派: Id 块设置 id_value 上下文; 后续块的 match_id 字段决定是否输出
///   - match_id == None: 始终输出
///   - match_id == Some(v): 仅当 v == id_value 时输出 (但所有块都消耗字节)
///
/// 变长字段: Length 块输出 length_value; Field 块的 length_ref 引用之, 决定 Bytes 类型长度
pub struct FrameParser {
    /// 块配置
    pub blocks: Vec<crate::DecoderBlockDef>,
    /// 附加输出端口开关
    pub enable_valid: bool,
    pub enable_frame_count: bool,
    pub enable_last_timestamp: bool,
    pub enable_fps: bool,

    /// 累积的字节缓冲区 (待解析)
    buf: Vec<u8>,
    /// 当前解析状态
    state: ParseState,
    /// 当前帧的 frame_start 在 buf 中的索引 (header 末尾位置 = 字段起始)
    frame_start: usize,
    /// 最近一次完整解析结果 (供 evaluate 读取)
    pub last_frame: ParsedFrame,
    /// 累计有效帧数
    pub frame_count: u64,
    /// 最近 N 帧的时间戳 (用于计算 fps, 滑动窗口)
    recent_timestamps: Vec<u64>,
}

impl FrameParser {
    pub fn new(
        blocks: Vec<crate::DecoderBlockDef>,
        enable_valid: bool,
        enable_frame_count: bool,
        enable_last_timestamp: bool,
        enable_fps: bool,
    ) -> Self {
        Self {
            blocks,
            enable_valid,
            enable_frame_count,
            enable_last_timestamp,
            enable_fps,
            buf: Vec::new(),
            state: ParseState::WaitForHeader,
            frame_start: 0,
            last_frame: ParsedFrame::default(),
            frame_count: 0,
            recent_timestamps: Vec::new(),
        }
    }

    /// 喂入新字节, 尝试解析完整帧
    ///
    /// 返回本次喂入解析出的完整帧列表 (可能 0 个, 1 个或多个)。
    /// 同时更新 `last_frame` 为最后一帧 (供 evaluate 读取)。
    pub fn feed(&mut self, data: &[u8], timestamp_us: u64) -> Vec<ParsedFrame> {
        self.buf.extend_from_slice(data);
        let mut frames = Vec::new();

        loop {
            match self.state {
                ParseState::WaitForHeader => {
                    let header_bytes = self.collect_header_bytes();
                    if header_bytes.is_empty() {
                        // 无 Header 块 — 直接尝试从 buf 开头解析
                        self.state = ParseState::ParseFields;
                        self.frame_start = 0;
                        continue;
                    }
                    match find_subsequence(&self.buf, &header_bytes) {
                        Some(pos) => {
                            // 丢弃 header 之前的字节
                            if pos > 0 {
                                self.buf.drain(0..pos);
                            }
                            self.frame_start = header_bytes.len();
                            self.state = ParseState::ParseFields;
                        }
                        None => {
                            // 未找到 header, 保留最后 header.len()-1 字节 (避免跨包截断)
                            let keep = header_bytes.len().saturating_sub(1);
                            if self.buf.len() > keep {
                                self.buf.drain(0..self.buf.len() - keep);
                            }
                            break;
                        }
                    }
                }
                ParseState::ParseFields => {
                    match self.try_parse_frame(timestamp_us) {
                        Some(result) => {
                            // 解析成功: 丢弃 consumed_bytes, 回到 WaitForHeader
                            let consumed = result.consumed_bytes;
                            // consumed 包括 header + 所有 blocks 消耗的字节
                            // frame_start 是 header 末尾, consumed 是从 buf 开头算的总消耗
                            let total_drain = self.frame_start + (consumed - self.frame_start);
                            let _ = total_drain; // 等价于 consumed, 保留语义
                            self.buf.drain(0..consumed);
                            self.state = ParseState::WaitForHeader;
                            self.frame_start = 0;

                            // 更新统计
                            self.frame_count += 1;
                            self.record_timestamp(timestamp_us);

                            // 缓存 last_frame
                            self.last_frame = result.frame.clone();

                            frames.push(result.frame);
                        }
                        None => {
                            // 字节不足, 等待更多数据
                            break;
                        }
                    }
                }
            }
        }

        frames
    }

    /// 一次性解析给定字节切片 (用于手动测试模式)
    ///
    /// 与 feed 不同, 此方法不依赖内部状态, 直接尝试从字节切片开头解析一帧。
    /// 如果字节切片以 header 开头则正常解析; 否则尝试在切片中查找 header。
    pub fn parse_once(&self, data: &[u8], timestamp_us: u64) -> Option<ParsedFrame> {
        self.parse_once_with_consumed(data, timestamp_us)
            .map(|(f, _)| f)
    }

    /// 一次性解析并返回 (ParsedFrame, consumed_bytes) — 供手动测试模式 UI 显示消耗字节数
    pub fn parse_once_with_consumed(
        &self,
        data: &[u8],
        timestamp_us: u64,
    ) -> Option<(ParsedFrame, usize)> {
        let header_bytes = self.collect_header_bytes();
        let start = if header_bytes.is_empty() {
            0
        } else {
            find_subsequence(data, &header_bytes)?
        };
        let frame_start = start + header_bytes.len();
        if frame_start > data.len() {
            return None;
        }
        let result = self.try_parse_frame_from(data, start, frame_start, timestamp_us)?;
        Some((result.frame, result.consumed_bytes))
    }

    /// 计算最近 fps (帧/秒)
    pub fn fps(&self) -> f32 {
        if self.recent_timestamps.len() < 2 {
            return 0.0;
        }
        let first = *self.recent_timestamps.first().unwrap_or(&0);
        let last = *self.recent_timestamps.last().unwrap_or(&0);
        let elapsed_us = last.saturating_sub(first);
        if elapsed_us == 0 {
            return 0.0;
        }
        let count = self.recent_timestamps.len() as f32;
        (count - 1.0) * 1_000_000.0 / elapsed_us as f32
    }

    /// blocks 配置是否与当前一致 (用于检测配置变化时重建)
    pub fn matches_config(
        &self,
        blocks: &[crate::DecoderBlockDef],
        enable_valid: bool,
        enable_frame_count: bool,
        enable_last_timestamp: bool,
        enable_fps: bool,
    ) -> bool {
        self.blocks.as_slice() == blocks
            && self.enable_valid == enable_valid
            && self.enable_frame_count == enable_frame_count
            && self.enable_last_timestamp == enable_last_timestamp
            && self.enable_fps == enable_fps
    }

    /// 收集所有 Header 块的字节 (按顺序拼接)
    /// 通常只有一个 Header 块; 多个则拼接 (用于多帧分派时不同 id 使用不同 header)
    fn collect_header_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for b in &self.blocks {
            if let crate::DecoderBlockDef::Header { hex, .. } = b {
                bytes.extend_from_slice(&parse_hex(hex));
            }
        }
        bytes
    }

    /// 尝试从 buf 当前状态解析一帧 (state == ParseFields 时调用)
    ///
    /// 返回 None 表示字节不足, 调用方应等待更多数据。
    fn try_parse_frame(&self, timestamp_us: u64) -> Option<ParseResult> {
        self.try_parse_frame_from(&self.buf, 0, self.frame_start, timestamp_us)
    }

    /// 从给定字节切片解析一帧 (核心解析逻辑)
    ///
    /// - `data`: 完整字节切片
    /// - `start`: 帧起始 (Header 开头) 在 data 中的索引
    /// - `frame_start`: Header 末尾 (= 字段起始) 在 data 中的索引
    fn try_parse_frame_from(
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
                crate::DecoderBlockDef::Header { .. } => {
                    // Header 已匹配, 跳过
                    continue;
                }
                crate::DecoderBlockDef::Length {
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
                crate::DecoderBlockDef::Id {
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
                crate::DecoderBlockDef::Field {
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
                    let n = if *field_type == crate::FieldType::Bytes {
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
                crate::DecoderBlockDef::Bitfield {
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
                crate::DecoderBlockDef::Checksum {
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
                        crate::DecoderChecksumCover::AllPrior => {
                            // 从 header 之后到当前 cursor
                            (frame_start, cursor)
                        }
                        crate::DecoderChecksumCover::Range => {
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
                        crate::DecoderChecksumPosition::Inline => {
                            // 校验字节在当前 cursor 位置
                            if cursor + cs_len > data.len() {
                                return None;
                            }
                            let b = &data[cursor..cursor + cs_len];
                            cursor += cs_len;
                            b.to_vec()
                        }
                        crate::DecoderChecksumPosition::Append => {
                            // 校验字节在帧末尾 (Tail 之前) — 此处简化为从 cursor 读取
                            if cursor + cs_len > data.len() {
                                return None;
                            }
                            let b = &data[cursor..cursor + cs_len];
                            cursor += cs_len;
                            b.to_vec()
                        }
                        crate::DecoderChecksumPosition::Prepend => {
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
                crate::DecoderBlockDef::Tail { hex, match_id, .. } => {
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
            }
        }

        // 计算消耗字节数 (从 start 到 cursor)
        let consumed_bytes = cursor - start;

        Some(ParseResult {
            frame: ParsedFrame {
                outputs,
                valid,
                timestamp_us,
                id_value,
            },
            consumed_bytes,
        })
    }

    /// 记录一帧时间戳并维护滑动窗口 (最多 60 个采样点, 约 1 秒 @ 60fps)
    fn record_timestamp(&mut self, ts: u64) {
        self.recent_timestamps.push(ts);
        if self.recent_timestamps.len() > 60 {
            self.recent_timestamps.remove(0);
        }
    }
}

// ============ 工具函数 ============

/// 判断块是否应执行 (基于 match_id 与 id_value)
fn block_should_execute(match_id: Option<i64>, id_value: Option<i64>) -> bool {
    match match_id {
        None => true,                   // 无 match_id → 始终执行
        Some(v) => id_value == Some(v), // 有 match_id → 仅当 id_value 匹配时执行
    }
}

/// 在 buf 中查找 subsequence
fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
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

/// 获取校验算法输出的字节长度
fn checksum_byte_len(algo: ChecksumAlgorithm) -> usize {
    match algo {
        ChecksumAlgorithm::None => 0,
        ChecksumAlgorithm::Sum8
        | ChecksumAlgorithm::Xor8
        | ChecksumAlgorithm::Crc8
        | ChecksumAlgorithm::Lrc => 1,
        ChecksumAlgorithm::Crc16Modbus | ChecksumAlgorithm::Crc16CCITT => 2,
        ChecksumAlgorithm::Crc32 => 4,
        ChecksumAlgorithm::Custom => 0, // Custom 暂不支持后端求值
    }
}

// ============ HEX 解析工具 ============

// ============ 帧解码器测试数据生成器 ============

/// 帧解码器测试数据生成器
///
/// 根据 [`DecoderBlockDef`] 配置反向编码字节序列, 使得编码后的字节能够被
/// [`FrameParser`] 解析, 并产生预期的端口输出值。
///
/// 用于端到端测试帧解码器: 先编码字节喂入 parser, 再断言解析结果与预期一致。
///
/// # 编码规则
///
/// | 块类型   | 编码方式 |
/// |----------|----------|
/// | Header   | 写入 `hex` 的原始字节 |
/// | Length   | 从 `field_values` 取端口名对应值, 按 `field_type` 编码为整数写入 |
/// | Id       | 从 `field_values` 取端口名对应值, 按 `field_type` 编码为整数写入 |
/// | Field    | 从 `field_values` 取 `port_name` 对应值, 按 `field_type` 编码写入 |
/// | Field(Bytes) | 写入 `length_ref` 引用长度个字节, 首字节 = 端口值 |
/// | Bitfield | 在 `byte_offset`/`bit_offset` 位置写入 `bit_length` 位 (MSB first) |
/// | Checksum | 对覆盖范围字节计算校验值, 写入指定位置 |
/// | Tail     | 写入 `hex` 的原始字节 |
///
/// # 示例
///
/// ```ignore
/// use vofa_next_nodes::frame_decoder::FrameDecoderTestData;
/// use vofa_next_nodes::{DecoderBlockDef, FieldType};
/// use std::collections::HashMap;
///
/// let blocks = vec![
///     DecoderBlockDef::Header { id: "h1".into(), hex: "AA".into(), match_id: None },
///     DecoderBlockDef::Field {
///         id: "f1".into(), field_type: FieldType::UInt16LE,
///         port_name: "ch0".into(), length_ref: None, match_id: None,
///     },
///     DecoderBlockDef::Tail { id: "t1".into(), hex: "BB".into(), match_id: None },
/// ];
///
/// let mut values = HashMap::new();
/// values.insert("ch0".to_string(), 258.0); // 0x0102
///
/// let bytes = FrameDecoderTestData::encode_frame(&blocks, &values);
/// assert_eq!(bytes, vec![0xAA, 0x02, 0x01, 0xBB]);
/// ```
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
    pub fn encode_frame(
        blocks: &[DecoderBlockDef],
        field_values: &HashMap<String, f32>,
    ) -> Vec<u8> {
        Self::encode_frame_with_checksums(blocks, field_values).0
    }

    /// 与 [`Self::encode_frame`] 相同, 但额外返回每个 Checksum 块的 (起始位置, 字节长度)。
    fn encode_frame_with_checksums(
        blocks: &[DecoderBlockDef],
        field_values: &HashMap<String, f32>,
    ) -> (Vec<u8>, Vec<(usize, usize)>) {
        use crate::DecoderChecksumCover::AllPrior;
        use crate::DecoderChecksumPosition::{Append, Inline, Prepend};

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
            position: crate::DecoderChecksumPosition,
            cs_len: usize, // 校验值字节长度
        }
        let mut checksums: Vec<CsRecord> = Vec::new();
        // 记录 frame_start = Header 末尾在 buf 中的位置
        let mut frame_start: usize = 0;
        // 跟踪 Id 块设置的 id_value, 用于 match_id 过滤
        let mut current_id_value: Option<i64> = None;
        for block in blocks {
            // match_id 过滤: 仅当当前帧 id_value 等于 match_id 时编码该块
            if let Some(expected) = block.match_id() {
                if current_id_value != Some(expected) {
                    continue;
                }
            }
            match block {
                DecoderBlockDef::Header { hex, .. } => {
                    frame_start = buf.len() + parse_hex(hex).len();
                    buf.extend_from_slice(&parse_hex(hex));
                }
                DecoderBlockDef::Length {
                    field_type,
                    port_name,
                    id,
                    ..
                } => {
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
                    current_id_value = Some(val as i64);
                    encode_int(&mut buf, *field_type, val);
                }
                DecoderBlockDef::Field {
                    field_type,
                    port_name,
                    length_ref,
                    ..
                } => {
                    let val = field_values.get(port_name).copied().unwrap_or(0.0);
                    if *field_type == crate::FieldType::Bytes {
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
                    ..
                } => {
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
                    ..
                } => {
                    // 先放置占位字节 (全 0), 第二遍计算后替换
                    let cs_len = checksum_byte_len(*algorithm);
                    let placeholder = vec![0u8; cs_len];
                    let record = match position {
                        Prepend => {
                            // 校验字节在 frame_start 位置
                            let pos = frame_start;
                            // 插入占位, 移动后续字节
                            let mut placeholder_clone = placeholder.clone();
                            placeholder_clone.extend_from_slice(&buf[pos..]);
                            buf.truncate(pos);
                            buf.extend_from_slice(&placeholder);
                            // 注意: 占位后需要把后面的字节再补上
                            // 简化处理: 先把整个 buf 往后推
                            // 更好的方式: 用 splice-like 操作
                            // 简单处理: 保存尾部, 追加占位, 再追加尾部
                            // 但由于我们已经写到 buf 了, 用更复杂的方式
                            // 先记下位置, 最后再处理
                            // 对于 Prepend, 占位在 frame_start 之后立即放置
                            // 简单实现: 先记下, 最后拼接
                            CsRecord {
                                buf_pos: pos,
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
                                crate::DecoderChecksumCover::Range => {
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
                DecoderBlockDef::Tail { hex, .. } => {
                    buf.extend_from_slice(&parse_hex(hex));
                }
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

        let positions = checksums.iter().map(|cs| (cs.buf_pos, cs.cs_len)).collect();
        (buf, positions)
    }

    /// 编码多帧字节序列 (拼接 `encode_frame` 结果)
    ///
    /// - `blocks`: 共享的帧解码块定义
    /// - `frames`: 每帧的端口值映射列表
    ///
    /// 每帧的时间戳依次递增 1000 微秒。
    /// 返回连续拼接的完整字节流, 可直接喂入 [`FrameParser::feed`]。
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
    /// 在 `encode_frame` 的基础上, 将最后一个校验字节取反。
    pub fn encode_frame_bad_checksum(
        blocks: &[DecoderBlockDef],
        field_values: &HashMap<String, f32>,
    ) -> Vec<u8> {
        let (mut data, checksum_positions) =
            Self::encode_frame_with_checksums(blocks, field_values);
        // 精确翻转最后一个 Checksum 块的校验字节 (保留 Tail 不变, 保证帧结构仍可解析,
        // 仅校验失败)
        if let Some(&(pos, len)) = checksum_positions.last() {
            for j in 0..len {
                if let Some(b) = data.get_mut(pos + j) {
                    *b = !*b;
                }
            }
        }
        data
    }
}

/// 按 field_type 将 u64 整数编码为字节, 追加到 buf
fn encode_int(buf: &mut Vec<u8>, ft: crate::FieldType, val: u64) {
    match ft {
        crate::FieldType::UInt8 | crate::FieldType::Int8 => {
            buf.push(val as u8);
        }
        crate::FieldType::UInt16LE | crate::FieldType::Int16LE => {
            buf.extend_from_slice(&(val as u16).to_le_bytes());
        }
        crate::FieldType::UInt16BE | crate::FieldType::Int16BE => {
            buf.extend_from_slice(&(val as u16).to_be_bytes());
        }
        crate::FieldType::UInt32LE | crate::FieldType::Int32LE => {
            buf.extend_from_slice(&(val as u32).to_le_bytes());
        }
        crate::FieldType::UInt32BE | crate::FieldType::Int32BE => {
            buf.extend_from_slice(&(val as u32).to_be_bytes());
        }
        crate::FieldType::Float32LE | crate::FieldType::Float32BE | crate::FieldType::Bytes => {
            // Float/Bytes 不适合整数编码: 写入 0 占位
            buf.push(val as u8);
        }
    }
}

/// 按 field_type 将 f32 值编码为字节, 追加到 buf
fn encode_float(buf: &mut Vec<u8>, ft: crate::FieldType, val: f32) {
    match ft {
        crate::FieldType::UInt8 | crate::FieldType::Int8 => {
            buf.push(val as u8);
        }
        crate::FieldType::UInt16LE | crate::FieldType::Int16LE => {
            buf.extend_from_slice(&(val as u16).to_le_bytes());
        }
        crate::FieldType::UInt16BE | crate::FieldType::Int16BE => {
            buf.extend_from_slice(&(val as u16).to_be_bytes());
        }
        crate::FieldType::UInt32LE | crate::FieldType::Int32LE => {
            buf.extend_from_slice(&(val as u32).to_le_bytes());
        }
        crate::FieldType::UInt32BE | crate::FieldType::Int32BE => {
            buf.extend_from_slice(&(val as u32).to_be_bytes());
        }
        crate::FieldType::Float32LE => {
            buf.extend_from_slice(&val.to_le_bytes());
        }
        crate::FieldType::Float32BE => {
            buf.extend_from_slice(&val.to_be_bytes());
        }
        crate::FieldType::Bytes => {
            // Bytes 类型: 写入首字节
            buf.push(val as u8);
        }
    }
}

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

    #[test]
    fn test_parse_hex_spaces() {
        assert_eq!(parse_hex("AA BB"), vec![0xAA, 0xBB]);
        assert_eq!(parse_hex("AABB"), vec![0xAA, 0xBB]);
        assert_eq!(parse_hex("aa bb"), vec![0xAA, 0xBB]);
        assert_eq!(parse_hex("0xAA 0xBB"), vec![0xAA, 0xBB]);
    }

    #[test]
    fn test_parse_hex_invalid() {
        assert_eq!(parse_hex("AAB"), Vec::<u8>::new()); // 奇数长度
        assert_eq!(parse_hex("ZZ"), Vec::<u8>::new()); // 非法字符
    }

    #[test]
    fn test_checksum_sum8() {
        let data = [0x01, 0x02, 0x03];
        let cs = ChecksumAlgorithm::Sum8.compute(&data, None);
        assert_eq!(cs, vec![0x06]); // 1+2+3=6
    }

    #[test]
    fn test_checksum_xor8() {
        let data = [0x01, 0x02, 0x03];
        let cs = ChecksumAlgorithm::Xor8.compute(&data, None);
        assert_eq!(cs, vec![0x00]); // 1^2^3=0
    }

    #[test]
    fn test_checksum_crc8() {
        // CRC-8/SMBUS: poly=0x07, init=0x00
        // "123456789" → 0xF4
        let data = b"123456789";
        let cs = ChecksumAlgorithm::Crc8.compute(data, None);
        assert_eq!(cs, vec![0xF4]);
    }

    #[test]
    fn test_checksum_crc16_modbus() {
        // CRC-16/Modbus: "123456789" → 0x4B37
        let data = b"123456789";
        let cs = ChecksumAlgorithm::Crc16Modbus.compute(data, None);
        assert_eq!(cs, vec![0x37, 0x4B]); // LE
    }

    #[test]
    fn test_checksum_crc16_ccitt() {
        // CRC-16/CCITT-FALSE: "123456789" → 0x29B1
        let data = b"123456789";
        let cs = ChecksumAlgorithm::Crc16CCITT.compute(data, None);
        assert_eq!(cs, vec![0x29, 0xB1]); // BE
    }

    #[test]
    fn test_checksum_crc32() {
        // CRC-32/ISO-HDLC: "123456789" → 0xCBF43926
        let data = b"123456789";
        let cs = ChecksumAlgorithm::Crc32.compute(data, None);
        assert_eq!(cs, vec![0x26, 0x39, 0xF4, 0xCB]); // LE
    }

    #[test]
    fn test_checksum_lrc() {
        // LRC: 0 - sum(data) mod 256
        let data = [0x01, 0x02, 0x03];
        let cs = ChecksumAlgorithm::Lrc.compute(&data, None);
        // 0 - 6 = 0xFA (mod 256, 二补码)
        assert_eq!(cs, vec![0xFA]);
    }

    #[test]
    fn test_checksum_verify() {
        let data = [0x01, 0x02, 0x03];
        // sum8 = 0x06
        assert!(ChecksumAlgorithm::Sum8.verify(&data, &[0x06], None));
        assert!(!ChecksumAlgorithm::Sum8.verify(&data, &[0x07], None));
    }

    #[test]
    fn test_fps_empty() {
        let p = FrameParser::new(Vec::new(), false, false, false, false);
        assert_eq!(p.fps(), 0.0);
    }

    #[test]
    fn test_matches_config() {
        let blocks = vec![];
        let p = FrameParser::new(blocks.clone(), true, false, false, false);
        assert!(p.matches_config(&blocks, true, false, false, false));
        assert!(!p.matches_config(&blocks, false, false, false, false));
    }

    // ============ FrameParser 端到端测试 ============

    use crate::{DecoderBlockDef, FieldType};

    fn header(id: &str, hex: &str) -> DecoderBlockDef {
        DecoderBlockDef::Header {
            id: id.to_string(),
            hex: hex.to_string(),
            match_id: None,
        }
    }

    fn tail(id: &str, hex: &str) -> DecoderBlockDef {
        DecoderBlockDef::Tail {
            id: id.to_string(),
            hex: hex.to_string(),
            match_id: None,
        }
    }

    fn field(id: &str, ft: FieldType, port: &str) -> DecoderBlockDef {
        DecoderBlockDef::Field {
            id: id.to_string(),
            field_type: ft,
            port_name: port.to_string(),
            length_ref: None,
            match_id: None,
        }
    }

    #[test]
    fn test_parse_fixed_length_frame() {
        // 帧: AA <uint16LE 0x0102> <uint16LE 0x0304> BB
        // 字节: AA 02 01 04 03 BB
        let blocks = vec![
            header("h1", "AA"),
            field("f1", FieldType::UInt16LE, "ch0"),
            field("f2", FieldType::UInt16LE, "ch1"),
            tail("t1", "BB"),
        ];
        let parser = FrameParser::new(blocks, false, false, false, false);

        let data = [0xAA, 0x02, 0x01, 0x04, 0x03, 0xBB];
        let result = parser.parse_once(&data, 1000).expect("应解析成功");

        assert_eq!(result.outputs.get("ch0"), Some(&258.0)); // 0x0102 = 258
        assert_eq!(result.outputs.get("ch1"), Some(&772.0)); // 0x0304 = 772
        assert!(result.valid);
        assert_eq!(result.id_value, None);
    }

    #[test]
    fn test_parse_with_checksum_sum8() {
        // 帧: AA <uint8 0x01> <sum8: 0x01> BB
        // sum8(0x01) = 0x01
        let blocks = vec![
            header("h1", "AA"),
            field("f1", FieldType::UInt8, "value"),
            DecoderBlockDef::Checksum {
                id: "cs1".to_string(),
                algorithm: ChecksumAlgorithm::Sum8,
                custom_script: None,
                cover: crate::DecoderChecksumCover::AllPrior,
                cover_start: None,
                cover_end: None,
                position: crate::DecoderChecksumPosition::Inline,
                match_id: None,
            },
            tail("t1", "BB"),
        ];
        let parser = FrameParser::new(blocks, false, false, false, false);

        // 正确校验: AA 01 01 BB
        let data_ok = [0xAA, 0x01, 0x01, 0xBB];
        let result = parser.parse_once(&data_ok, 1000).expect("应解析成功");
        assert!(result.valid);
        assert_eq!(result.outputs.get("value"), Some(&1.0));

        // 错误校验: AA 01 02 BB (sum8 应为 0x01, 实际为 0x02)
        let data_bad = [0xAA, 0x01, 0x02, 0xBB];
        let result_bad = parser.parse_once(&data_bad, 1000).expect("应解析成功");
        assert!(!result_bad.valid);
    }

    #[test]
    fn test_parse_variable_length_frame() {
        // 帧: AA <uint8 length=N> <N bytes data> BB
        // 示例: AA 03 11 22 33 BB (length=3, data=[0x11, 0x22, 0x33])
        let blocks = vec![
            header("h1", "AA"),
            DecoderBlockDef::Length {
                id: "len1".to_string(),
                field_type: FieldType::UInt8,
                port_name: Some("length".to_string()),
                unit: Some(crate::LengthUnit::Bytes),
                match_id: None,
            },
            DecoderBlockDef::Field {
                id: "f1".to_string(),
                field_type: FieldType::Bytes,
                port_name: "data".to_string(),
                length_ref: Some("len1".to_string()),
                match_id: None,
            },
            tail("t1", "BB"),
        ];
        let parser = FrameParser::new(blocks, false, false, false, false);

        let data = [0xAA, 0x03, 0x11, 0x22, 0x33, 0xBB];
        let result = parser.parse_once(&data, 1000).expect("应解析成功");
        assert_eq!(result.outputs.get("length"), Some(&3.0));
        // Bytes 类型输出第一字节
        assert_eq!(result.outputs.get("data"), Some(&17.0)); // 0x11 = 17
    }

    #[test]
    fn test_parse_multi_frame_dispatch() {
        // 帧: AA <uint8 id> <uint8 value> BB
        // id=1: value 输出到 "type_a" 端口
        // id=2: value 输出到 "type_b" 端口
        let blocks = vec![
            header("h1", "AA"),
            DecoderBlockDef::Id {
                id: "id1".to_string(),
                field_type: FieldType::UInt8,
                port_name: Some("id_value".to_string()),
            },
            DecoderBlockDef::Field {
                id: "f_a".to_string(),
                field_type: FieldType::UInt8,
                port_name: "type_a".to_string(),
                length_ref: None,
                match_id: Some(1),
            },
            DecoderBlockDef::Field {
                id: "f_b".to_string(),
                field_type: FieldType::UInt8,
                port_name: "type_b".to_string(),
                length_ref: None,
                match_id: Some(2),
            },
            tail("t1", "BB"),
        ];
        let parser = FrameParser::new(blocks, false, false, false, false);

        // id=1 帧: AA 01 42 BB → type_a=0x42=66
        let data_a = [0xAA, 0x01, 0x42, 0xBB];
        let result_a = parser.parse_once(&data_a, 1000).expect("应解析成功");
        assert_eq!(result_a.id_value, Some(1));
        assert_eq!(result_a.outputs.get("id_value"), Some(&1.0));
        assert_eq!(result_a.outputs.get("type_a"), Some(&66.0));
        assert!(!result_a.outputs.contains_key("type_b"));

        // id=2 帧: AA 02 99 BB → type_b=0x99=153
        let data_b = [0xAA, 0x02, 0x99, 0xBB];
        let result_b = parser.parse_once(&data_b, 2000).expect("应解析成功");
        assert_eq!(result_b.id_value, Some(2));
        assert_eq!(result_b.outputs.get("type_b"), Some(&153.0));
        assert!(!result_b.outputs.contains_key("type_a"));
    }

    #[test]
    fn test_parse_bitfield() {
        // 帧: AA <byte 0xAB=10101011> BB
        // Bitfield 不消耗 cursor, byte_offset 相对 frame_start (header 之后)
        // 需要一个 Field 块消耗字节, 让 cursor 前进到 Tail 位置
        let blocks = vec![
            header("h1", "AA"),
            // Field 消耗 1 字节 (0xAB), cursor 前进到 2
            field("f1", FieldType::UInt8, "raw_byte"),
            // Bitfield 从 frame_start=1 读取 (相对 header 之后)
            DecoderBlockDef::Bitfield {
                id: "bf1".to_string(),
                byte_offset: 0,
                bit_offset: 0,
                bit_length: 4,
                is_signed: false,
                port_name: "high_nibble".to_string(),
                match_id: None,
            },
            DecoderBlockDef::Bitfield {
                id: "bf2".to_string(),
                byte_offset: 0,
                bit_offset: 4,
                bit_length: 4,
                is_signed: false,
                port_name: "low_nibble".to_string(),
                match_id: None,
            },
            tail("t1", "BB"),
        ];
        let parser = FrameParser::new(blocks, false, false, false, false);

        // AA AB BB → raw_byte=171, high=0xA=10, low=0xB=11
        let data = [0xAA, 0xAB, 0xBB];
        let result = parser.parse_once(&data, 1000).expect("应解析成功");
        assert_eq!(result.outputs.get("raw_byte"), Some(&171.0));
        assert_eq!(result.outputs.get("high_nibble"), Some(&10.0));
        assert_eq!(result.outputs.get("low_nibble"), Some(&11.0));
    }

    #[test]
    fn test_parse_bitfield_signed() {
        // 帧: AA <byte 0xA0=10100000> BB
        // Bitfield: byteOffset=0, bitOffset=0, bitLength=4, signed → 0b1010 = -6 (二补码)
        let blocks = vec![
            header("h1", "AA"),
            field("f1", FieldType::UInt8, "raw_byte"),
            DecoderBlockDef::Bitfield {
                id: "bf1".to_string(),
                byte_offset: 0,
                bit_offset: 0,
                bit_length: 4,
                is_signed: true,
                port_name: "val".to_string(),
                match_id: None,
            },
            tail("t1", "BB"),
        ];
        let parser = FrameParser::new(blocks, false, false, false, false);

        // AA A0 BB → bitfield=0b1010 (4位有符号) = -6
        let data = [0xAA, 0xA0, 0xBB];
        let result = parser.parse_once(&data, 1000).expect("应解析成功");
        assert_eq!(result.outputs.get("val"), Some(&-6.0));
    }

    #[test]
    fn test_feed_multi_frame_in_one_chunk() {
        // 一次喂入两个完整帧, 应解析出 2 个 ParsedFrame
        let blocks = vec![
            header("h1", "AA"),
            field("f1", FieldType::UInt8, "v"),
            tail("t1", "BB"),
        ];
        let mut parser = FrameParser::new(blocks, false, false, false, false);

        // 两帧: AA 01 BB AA 02 BB
        let data = [0xAA, 0x01, 0xBB, 0xAA, 0x02, 0xBB];
        let frames = parser.feed(&data, 1000);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].outputs.get("v"), Some(&1.0));
        assert_eq!(frames[1].outputs.get("v"), Some(&2.0));
        assert_eq!(parser.frame_count, 2);
    }

    #[test]
    fn test_feed_split_across_chunks() {
        // 帧跨多个数据包到达, 应正确累积解析
        let blocks = vec![
            header("h1", "AA"),
            field("f1", FieldType::UInt16LE, "v"),
            tail("t1", "BB"),
        ];
        let mut parser = FrameParser::new(blocks, false, false, false, false);

        // 第一包: AA 01 (不完整)
        let f1 = parser.feed(&[0xAA, 0x01], 1000);
        assert_eq!(f1.len(), 0);

        // 第二包: 00 BB (完整帧: AA 01 00 BB → v=0x0001=1)
        let f2 = parser.feed(&[0x00, 0xBB], 2000);
        assert_eq!(f2.len(), 1);
        assert_eq!(f2[0].outputs.get("v"), Some(&1.0));
        assert_eq!(parser.frame_count, 1);
    }

    #[test]
    fn test_feed_with_garbage_before_header() {
        // 数据前有垃圾字节, 应自动跳过找到 header
        let blocks = vec![
            header("h1", "AA"),
            field("f1", FieldType::UInt8, "v"),
            tail("t1", "BB"),
        ];
        let mut parser = FrameParser::new(blocks, false, false, false, false);

        // 垃圾 + 完整帧: FF FF FF AA 42 BB
        let data = [0xFF, 0xFF, 0xFF, 0xAA, 0x42, 0xBB];
        let frames = parser.feed(&data, 1000);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].outputs.get("v"), Some(&66.0));
    }

    #[test]
    fn test_parse_no_header() {
        // 无 Header 块 — 直接从开头解析
        let blocks = vec![field("f1", FieldType::UInt8, "v"), tail("t1", "BB")];
        let parser = FrameParser::new(blocks, false, false, false, false);

        let data = [0x42, 0xBB];
        let result = parser.parse_once(&data, 1000).expect("应解析成功");
        assert_eq!(result.outputs.get("v"), Some(&66.0));
    }

    #[test]
    fn test_parse_tail_mismatch_returns_none() {
        // Tail 不匹配 → 返回 None (等待重新查找 header)
        let blocks = vec![
            header("h1", "AA"),
            field("f1", FieldType::UInt8, "v"),
            tail("t1", "BB"),
        ];
        let parser = FrameParser::new(blocks, false, false, false, false);

        // AA 42 CC (Tail 应为 BB, 实际 CC)
        let data = [0xAA, 0x42, 0xCC];
        assert!(parser.parse_once(&data, 1000).is_none());
    }

    // ============ FrameDecoderTestData 闭环测试 ============
    //
    // encode_frame / encode_frames  →  FrameParser（parse / feed）
    // 验证编码→解析闭环: 输出值应与输入一致

    use super::FrameDecoderTestData;
    use std::collections::HashMap;

    /// 编码固定格式帧, 再用 parser 解析, 验证 round-trip
    #[test]
    fn test_encode_parse_roundtrip_fixed() {
        let blocks = vec![
            header("h1", "AA"),
            field("f1", FieldType::UInt16LE, "ch0"),
            field("f2", FieldType::UInt16LE, "ch1"),
            tail("t1", "BB"),
        ];
        let mut values = HashMap::new();
        values.insert("ch0".to_string(), 258.0); // 0x0102
        values.insert("ch1".to_string(), 772.0); // 0x0304

        let bytes = FrameDecoderTestData::encode_frame(&blocks, &values);
        assert_eq!(bytes, vec![0xAA, 0x02, 0x01, 0x04, 0x03, 0xBB]);

        let parser = FrameParser::new(blocks, false, false, false, false);
        let result = parser.parse_once(&bytes, 1000).expect("应解析成功");
        assert_eq!(result.outputs.get("ch0"), Some(&258.0));
        assert_eq!(result.outputs.get("ch1"), Some(&772.0));
        assert!(result.valid);
    }

    /// Checksum Sum8 + Inline 闭环
    #[test]
    fn test_encode_parse_roundtrip_checksum() {
        let blocks = vec![
            header("h1", "AA"),
            field("f1", FieldType::UInt8, "value"),
            DecoderBlockDef::Checksum {
                id: "cs1".to_string(),
                algorithm: ChecksumAlgorithm::Sum8,
                custom_script: None,
                cover: crate::DecoderChecksumCover::AllPrior,
                cover_start: None,
                cover_end: None,
                position: crate::DecoderChecksumPosition::Inline,
                match_id: None,
            },
            tail("t1", "BB"),
        ];
        let mut values = HashMap::new();
        values.insert("value".to_string(), 42.0);

        let bytes = FrameDecoderTestData::encode_frame(&blocks, &values);
        // AA 2A SUM8 BB → sum8(0x2A) = 0x2A
        assert_eq!(bytes, vec![0xAA, 0x2A, 0x2A, 0xBB]);

        let parser = FrameParser::new(blocks, false, false, false, false);
        let result = parser.parse_once(&bytes, 1000).expect("应解析成功");
        assert!(result.valid);
        assert_eq!(result.outputs.get("value"), Some(&42.0));
    }

    /// 变长帧 (Length + Bytes) 闭环
    #[test]
    fn test_encode_parse_roundtrip_variable_length() {
        let blocks = vec![
            header("h1", "AA"),
            DecoderBlockDef::Length {
                id: "len1".to_string(),
                field_type: FieldType::UInt8,
                port_name: Some("length".to_string()),
                unit: Some(crate::LengthUnit::Bytes),
                match_id: None,
            },
            DecoderBlockDef::Field {
                id: "f1".to_string(),
                field_type: FieldType::Bytes,
                port_name: "data".to_string(),
                length_ref: Some("len1".to_string()),
                match_id: None,
            },
            tail("t1", "BB"),
        ];
        let mut values = HashMap::new();
        values.insert("length".to_string(), 3.0);
        values.insert("data".to_string(), 17.0); // 首字节 = 0x11

        let bytes = FrameDecoderTestData::encode_frame(&blocks, &values);
        // AA 03 11 12 13 BB
        assert_eq!(bytes, vec![0xAA, 0x03, 0x11, 0x12, 0x13, 0xBB]);

        let parser = FrameParser::new(blocks, false, false, false, false);
        let result = parser.parse_once(&bytes, 1000).expect("应解析成功");
        assert_eq!(result.outputs.get("length"), Some(&3.0));
        assert_eq!(result.outputs.get("data"), Some(&17.0));
    }

    /// 多帧分派 (Id + match_id) 闭环
    #[test]
    fn test_encode_parse_roundtrip_multi_frame() {
        let blocks = vec![
            header("h1", "AA"),
            DecoderBlockDef::Id {
                id: "id1".to_string(),
                field_type: FieldType::UInt8,
                port_name: Some("id_value".to_string()),
            },
            DecoderBlockDef::Field {
                id: "f_a".to_string(),
                field_type: FieldType::UInt8,
                port_name: "type_a".to_string(),
                length_ref: None,
                match_id: Some(1),
            },
            DecoderBlockDef::Field {
                id: "f_b".to_string(),
                field_type: FieldType::UInt8,
                port_name: "type_b".to_string(),
                length_ref: None,
                match_id: Some(2),
            },
            tail("t1", "BB"),
        ];

        // id=1 帧
        let mut values_a = HashMap::new();
        values_a.insert("id_value".to_string(), 1.0);
        values_a.insert("type_a".to_string(), 66.0);
        let bytes_a = FrameDecoderTestData::encode_frame(&blocks, &values_a);
        assert_eq!(bytes_a, vec![0xAA, 0x01, 0x42, 0xBB]);

        // id=2 帧
        let mut values_b = HashMap::new();
        values_b.insert("id_value".to_string(), 2.0);
        values_b.insert("type_b".to_string(), 99.0);
        let bytes_b = FrameDecoderTestData::encode_frame(&blocks, &values_b);
        assert_eq!(bytes_b, vec![0xAA, 0x02, 0x63, 0xBB]);

        let parser = FrameParser::new(blocks, false, false, false, false);
        let result_a = parser.parse_once(&bytes_a, 1000).expect("应解析成功");
        assert_eq!(result_a.id_value, Some(1));
        assert_eq!(result_a.outputs.get("type_a"), Some(&66.0));
        assert!(!result_a.outputs.contains_key("type_b"));

        let result_b = parser.parse_once(&bytes_b, 2000).expect("应解析成功");
        assert_eq!(result_b.id_value, Some(2));
        assert_eq!(result_b.outputs.get("type_b"), Some(&99.0));
        assert!(!result_b.outputs.contains_key("type_a"));
    }

    /// Bitfield 闭环
    #[test]
    fn test_encode_parse_roundtrip_bitfield() {
        let blocks = vec![
            header("h1", "AA"),
            field("f1", FieldType::UInt8, "raw_byte"),
            DecoderBlockDef::Bitfield {
                id: "bf1".to_string(),
                byte_offset: 0,
                bit_offset: 0,
                bit_length: 4,
                is_signed: false,
                port_name: "high_nibble".to_string(),
                match_id: None,
            },
            DecoderBlockDef::Bitfield {
                id: "bf2".to_string(),
                byte_offset: 0,
                bit_offset: 4,
                bit_length: 4,
                is_signed: false,
                port_name: "low_nibble".to_string(),
                match_id: None,
            },
            tail("t1", "BB"),
        ];
        let mut values = HashMap::new();
        values.insert("raw_byte".to_string(), 171.0); // 0xAB
        values.insert("high_nibble".to_string(), 0xA_u32 as f32); // = 10
        values.insert("low_nibble".to_string(), 0xB_u32 as f32); // = 11

        let bytes = FrameDecoderTestData::encode_frame(&blocks, &values);
        // AA AB BB
        assert_eq!(bytes, vec![0xAA, 0xAB, 0xBB]);

        let parser = FrameParser::new(blocks, false, false, false, false);
        let result = parser.parse_once(&bytes, 1000).expect("应解析成功");
        assert_eq!(result.outputs.get("raw_byte"), Some(&171.0));
        assert_eq!(result.outputs.get("high_nibble"), Some(&10.0));
        assert_eq!(result.outputs.get("low_nibble"), Some(&11.0));
    }

    /// encode_frames 多帧拼接 → feed 一次多帧
    #[test]
    fn test_encode_frames_roundtrip() {
        let blocks = vec![
            header("h1", "AA"),
            field("f1", FieldType::UInt8, "v"),
            tail("t1", "BB"),
        ];

        let mut f1 = HashMap::new();
        f1.insert("v".to_string(), 1.0);
        let mut f2 = HashMap::new();
        f2.insert("v".to_string(), 2.0);
        let mut f3 = HashMap::new();
        f3.insert("v".to_string(), 3.0);

        let all_bytes = FrameDecoderTestData::encode_frames(&blocks, &[f1, f2, f3]);
        assert_eq!(
            all_bytes,
            vec![0xAA, 0x01, 0xBB, 0xAA, 0x02, 0xBB, 0xAA, 0x03, 0xBB]
        );

        let mut parser = FrameParser::new(blocks, false, false, false, false);
        let frames = parser.feed(&all_bytes, 1000);
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0].outputs.get("v"), Some(&1.0));
        assert_eq!(frames[1].outputs.get("v"), Some(&2.0));
        assert_eq!(frames[2].outputs.get("v"), Some(&3.0));
    }

    /// Checksum 错误检测 (bad_checksum 生成 + 解析验证)
    #[test]
    fn test_encode_bad_checksum_detected() {
        let blocks = vec![
            header("h1", "AA"),
            field("f1", FieldType::UInt8, "value"),
            DecoderBlockDef::Checksum {
                id: "cs1".to_string(),
                algorithm: ChecksumAlgorithm::Sum8,
                custom_script: None,
                cover: crate::DecoderChecksumCover::AllPrior,
                cover_start: None,
                cover_end: None,
                position: crate::DecoderChecksumPosition::Inline,
                match_id: None,
            },
            tail("t1", "BB"),
        ];

        let mut values = HashMap::new();
        values.insert("value".to_string(), 5.0);

        let bytes_bad = FrameDecoderTestData::encode_frame_bad_checksum(&blocks, &values);
        let parser = FrameParser::new(blocks, false, false, false, false);
        let result = parser.parse_once(&bytes_bad, 1000).expect("应解析成功");
        assert!(!result.valid, "校验字节错误应导致 valid=false");
    }

    /// encode_frame_with_id 便捷方法
    #[test]
    fn test_encode_with_id_roundtrip() {
        let blocks = vec![
            header("h1", "AA"),
            DecoderBlockDef::Id {
                id: "id1".to_string(),
                field_type: FieldType::UInt8,
                port_name: Some("id_value".to_string()),
            },
            field("f1", FieldType::UInt8, "value"),
            tail("t1", "BB"),
        ];

        let mut values = HashMap::new();
        values.insert("value".to_string(), 77.0);

        let bytes = FrameDecoderTestData::encode_frame_with_id(&blocks, 3, &values);
        // AA 03 4D BB
        assert_eq!(bytes, vec![0xAA, 0x03, 0x4D, 0xBB]);

        let parser = FrameParser::new(blocks, false, false, false, false);
        let result = parser.parse_once(&bytes, 1000).expect("应解析成功");
        assert_eq!(result.id_value, Some(3));
        assert_eq!(result.outputs.get("id_value"), Some(&3.0));
        assert_eq!(result.outputs.get("value"), Some(&77.0));
    }
}
