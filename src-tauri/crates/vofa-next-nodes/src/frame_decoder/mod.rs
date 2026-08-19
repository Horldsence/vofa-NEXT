//! # 帧解码状态机 (FrameDecoder)
//!
//! 镜像前端 `DecoderBlock` 块列表, 后端实现字节流 → 帧解析 → 输出端口值。
//!
//! 跨帧持久化: 由 data_loop 通过 `decoder_states: HashMap<widget_id, FrameParser>` 管理,
//! 与 `filter_states` 模式一致 — 节点首次出现时创建, 配置变化时重建。
//!
//! 状态机阶段:
//! 1. WAIT_FOR_HEADER: 累积字节, 匹配 Header.hex 后进入 PARSE_FIELDS
//! 2. PARSE_FIELDS: 按 blocks 顺序读取 Length/Id/Field/Bitfield/Checksum/Tail
//! 3. 任何阶段失败 → 回到 WAIT_FOR_HEADER, 丢弃已读字节, 重新匹配帧头
//!
//! 多帧分派: Id 块设置 id_value 上下文, 后续块的 match_id 字段决定是否执行
//! 变长字段: Length 块输出 length_value, Field 块的 length_ref 引用之, 决定 Bytes 类型长度

mod blocks;
mod test_data;

use std::collections::HashMap;

use crate::decoder_block::DecoderBlockDef;

pub use blocks::parse_hex;
pub use test_data::FrameDecoderTestData;

/// 校验算法类型已迁移至 `vofa-next-core::schema` (纯数据 + 纯函数求值);
/// 此处 re-export 保持 `frame_decoder::ChecksumAlgorithm` 引用不变。
pub use vofa_next_core::ChecksumAlgorithm;

#[cfg(test)]
mod roundtrip_tests;
#[cfg(test)]
mod tests;

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
    /// 本帧消耗的原始字节 (header 至末尾, 供前端 RawData 旁路通道显示)
    /// 默认空; 仅在 feed() 解析成功后填充, 不序列化到前端
    pub raw_bytes: Vec<u8>,
}

/// 内部解析结果 (含消耗字节数)
#[derive(Debug, Clone)]
pub(crate) struct ParseResult {
    pub(crate) frame: ParsedFrame,
    /// 本帧消耗的字节数 (包括 header + 所有 blocks 消耗的字节)
    pub(crate) consumed_bytes: usize,
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
/// 2. ParseFields: 按 blocks 顺序解析 (跳过 Header, 逐块求值见 blocks.rs)
/// 3. 解析完成: 丢弃 consumed_bytes, 回到 WaitForHeader
pub struct FrameParser {
    /// 块配置
    pub blocks: Vec<DecoderBlockDef>,
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
        blocks: Vec<DecoderBlockDef>,
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
        // 消费游标: 循环内只推进 base, 结束后一次性 drain
        // (原实现逐帧 drain(0..consumed), 大批次下 O(n²) memmove)
        let mut base = 0usize;

        loop {
            match self.state {
                ParseState::WaitForHeader => {
                    let header_bytes = self.collect_header_bytes();
                    if header_bytes.is_empty() {
                        // 无 Header 块 — 直接尝试从当前位置解析
                        self.state = ParseState::ParseFields;
                        self.frame_start = 0;
                        continue;
                    }
                    match blocks::find_subsequence(&self.buf[base..], &header_bytes) {
                        Some(pos) => {
                            // 跳过 header 之前的字节
                            base += pos;
                            self.frame_start = header_bytes.len();
                            self.state = ParseState::ParseFields;
                        }
                        None => {
                            // 未找到 header, 保留最后 header.len()-1 字节 (避免跨包截断)
                            let keep = header_bytes.len().saturating_sub(1);
                            base = base.max(self.buf.len().saturating_sub(keep));
                            break;
                        }
                    }
                }
                ParseState::ParseFields => {
                    match self.try_parse_frame_from(
                        &self.buf[base..],
                        0,
                        self.frame_start,
                        timestamp_us,
                    ) {
                        Some(mut result) => {
                            let consumed = result.consumed_bytes;
                            // 捕获本帧消耗的原始字节 (header 至末尾), 供旁路 RawData 通道使用
                            result.frame.raw_bytes = self.buf[base..base + consumed].to_vec();
                            base += consumed;
                            self.state = ParseState::WaitForHeader;
                            self.frame_start = 0;

                            self.frame_count += 1;
                            self.record_timestamp(timestamp_us);
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

        if base > 0 {
            self.buf.drain(..base);
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
            blocks::find_subsequence(data, &header_bytes)?
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
        blocks: &[DecoderBlockDef],
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
            if let DecoderBlockDef::Header { hex, .. } = b {
                bytes.extend_from_slice(&parse_hex(hex));
            }
        }
        bytes
    }

    /// 记录一帧时间戳并维护滑动窗口 (最多 60 个采样点, 约 1 秒 @ 60fps)
    fn record_timestamp(&mut self, ts: u64) {
        self.recent_timestamps.push(ts);
        if self.recent_timestamps.len() > 60 {
            self.recent_timestamps.remove(0);
        }
    }
}
