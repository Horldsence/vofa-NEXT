use std::time::{SystemTime, UNIX_EPOCH};

use can_types::CanFrame;
use protocol_engine::{split_at_boundaries, FeedOutput, ProtocolEngine};

/// candleLight (GSUSB) 二进制帧大小 (字节)
pub const CAND_FRAME_SIZE: usize = 24;
/// RX 帧命令 ID
pub const CAND_CMD_RX: u8 = 0x11;
/// TX 帧命令 ID
pub const CAND_CMD_TX: u8 = 0x12;
/// CAN ID 扩展帧标志位 (EFF)
pub const CAND_ID_EFF: u32 = 1 << 29;
/// CAN ID 远程帧标志位 (RTR)
pub const CAND_ID_RTR: u32 = 1 << 30;
/// CAN ID 掩码 (低 29 位有效)
pub const CAND_ID_MASK: u32 = 0x1FFFFFFF;
/// candleLight (GSUSB) 二进制协议引擎
///
/// 帧格式 (24 字节):
/// - offset 0: cmd_id (0x11 = RX_FRAME, 0x12 = TX_FRAME)
/// - offset 1: channel
/// - offset 2-3: reserved
/// - offset 4-7: timestamp_us (u32 LE, 1us 分辨率, 设备时钟)
/// - offset 8-11: CAN ID (u32 LE, bit 29=EFF, bit 30=RTR, bit 31=ERR)
/// - offset 12: DLC (低 4 位)
/// - offset 13-15: reserved
/// - offset 16-23: 8 字节数据 (不足 8 字节补 0)
///
/// 注: timestamp 字段为设备时钟 (可能从 0 开始), 此处简化处理,
/// 直接用系统 now_us() 作为帧时间戳 (与 slcan 一致), 忽略设备时间戳字段。
pub struct CandleEngine {
    /// 接收缓冲, 按 24 字节边界解析
    buf: Vec<u8>,
}

impl CandleEngine {
    pub fn new() -> Self {
        Self {
            buf: Vec::with_capacity(64),
        }
    }

    /// 当前系统时间 (微秒)
    fn now_us() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_micros()).unwrap_or(0))
    }
}

impl ProtocolEngine for CandleEngine {
    fn feed(&mut self, data: &[u8]) -> FeedOutput {
        // candleLight 不产生 DataFrame, 只产生 CanFrame
        self.buf.extend_from_slice(data);
        let mut frames = Vec::new();
        // 单趟按 24 字节边界解析, 结束后一次性 drain
        // (原实现逐帧 drain(..CAND_FRAME_SIZE), 大批次下 O(n²) memmove)
        let consumed = self.buf.len() / CAND_FRAME_SIZE * CAND_FRAME_SIZE;
        for off in (0..consumed).step_by(CAND_FRAME_SIZE) {
            let pkt: &[u8] = &self.buf[off..off + CAND_FRAME_SIZE];
            let cmd_id = pkt[0];
            // 跳过非帧命令 (如设置波特率响应 0x01 等)
            if cmd_id != CAND_CMD_RX && cmd_id != CAND_CMD_TX {
                continue;
            }
            let can_id_raw = u32::from_le_bytes([pkt[8], pkt[9], pkt[10], pkt[11]]);
            let dlc = pkt[12] & 0x0F;
            let extended = (can_id_raw & CAND_ID_EFF) != 0;
            let rtr = (can_id_raw & CAND_ID_RTR) != 0;
            let id = can_id_raw & CAND_ID_MASK;
            let data_bytes = pkt[16..16 + dlc as usize].to_vec();
            let direction = if cmd_id == CAND_CMD_TX {
                can_types::CanDirection::Tx
            } else {
                can_types::CanDirection::Rx
            };
            frames.push(CanFrame {
                timestamp: Self::now_us(),
                id,
                extended,
                rtr,
                dlc,
                data: data_bytes,
                direction,
            });
        }
        if consumed > 0 {
            self.buf.drain(..consumed);
        }
        FeedOutput::from_can_frames(frames)
    }

    fn encode_can(&mut self, frame: &CanFrame) -> Vec<u8> {
        let mut pkt = [0u8; CAND_FRAME_SIZE];
        pkt[0] = CAND_CMD_TX;
        // channel 在传输层处理, 这里设 0
        let mut can_id_raw = frame.id & CAND_ID_MASK;
        if frame.extended {
            can_id_raw |= CAND_ID_EFF;
        }
        if frame.rtr {
            can_id_raw |= CAND_ID_RTR;
        }
        pkt[8..12].copy_from_slice(&can_id_raw.to_le_bytes());
        pkt[12] = frame.dlc & 0x0F;
        // 数据填入 offset 16-23 (最多 8 字节)
        for (i, &b) in frame.data.iter().enumerate().take(8) {
            pkt[16 + i] = b;
        }
        pkt.to_vec()
    }

    fn encode_channel(&mut self, _channel: usize, value: f32) -> Vec<u8> {
        format!("{value:.6}\n").into_bytes()
    }

    fn encode_channels(&mut self, values: &[f32]) -> Vec<u8> {
        let s: Vec<String> = values.iter().map(|v| format!("{v:.6}")).collect();
        format!("{}\n", s.join(",")).into_bytes()
    }

    fn name(&self) -> &'static str {
        "CandleLight"
    }

    fn encode_frame(&mut self, _frame: &vofa_core::DataFrame) -> Vec<u8> {
        // CAN 协议无法从 DataFrame 重编码 (需走 encode_can), 语义不符
        Vec::new()
    }

    fn split_aligned(&self, data: &[u8], workers: usize) -> Option<Vec<std::ops::Range<usize>>> {
        // 边界 = 24 字节倍数位置 (调用方保证 data 起点帧对齐: pending 前置拼接)
        let n = data.len() / CAND_FRAME_SIZE;
        let boundaries: Vec<usize> = (1..=n).map(|i| i * CAND_FRAME_SIZE).collect();
        Some(split_at_boundaries(&boundaries, workers))
    }

    fn take_pending(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.buf)
    }

    fn new_worker(&self) -> Box<dyn ProtocolEngine> {
        Box::new(Self::new())
    }
}

impl Default for CandleEngine {
    fn default() -> Self {
        Self::new()
    }
}