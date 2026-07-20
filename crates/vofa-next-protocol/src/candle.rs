use std::time::{SystemTime, UNIX_EPOCH};
use vofa_next_core::{CanDirection, CanFrame, DataFrame};

use crate::engine::ProtocolEngine;

/// candleLight (GSUSB) 二进制帧大小 (字节)
const CAND_FRAME_SIZE: usize = 24;
/// RX 帧命令 ID
const CAND_CMD_RX: u8 = 0x11;
/// TX 帧命令 ID
const CAND_CMD_TX: u8 = 0x12;
/// CAN ID 扩展帧标志位 (EFF)
const CAND_ID_EFF: u32 = 1 << 29;
/// CAN ID 远程帧标志位 (RTR)
const CAND_ID_RTR: u32 = 1 << 30;
/// CAN ID 掩码 (低 29 位为实际 ID)
const CAND_ID_MASK: u32 = 0x1FFFFFFF;

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
    fn now_us(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0)
    }
}

impl ProtocolEngine for CandleEngine {
    fn feed(&mut self, _data: &[u8]) -> Vec<DataFrame> {
        // candleLight 不产生 DataFrame, 只产生 CanFrame (通过 feed_can)
        Vec::new()
    }

    fn feed_can(&mut self, data: &[u8]) -> Vec<CanFrame> {
        self.buf.extend_from_slice(data);
        let mut frames = Vec::new();
        while self.buf.len() >= CAND_FRAME_SIZE {
            // 取前 24 字节作为一个完整帧
            let pkt: [u8; CAND_FRAME_SIZE] = self.buf[..CAND_FRAME_SIZE].try_into().unwrap();
            self.buf.drain(..CAND_FRAME_SIZE);
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
                CanDirection::Tx
            } else {
                CanDirection::Rx
            };
            frames.push(CanFrame {
                timestamp: self.now_us(),
                id,
                extended,
                rtr,
                dlc,
                data: data_bytes,
                direction,
            });
        }
        frames
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
        format!("{:.6}\n", value).into_bytes()
    }

    fn encode_channels(&mut self, values: &[f32]) -> Vec<u8> {
        let s: Vec<String> = values.iter().map(|v| format!("{:.6}", v)).collect();
        format!("{}\n", s.join(",")).into_bytes()
    }

    fn name(&self) -> &str {
        "CandleLight"
    }
}

impl Default for CandleEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一个 24 字节 RX 帧
    fn make_rx_frame(cmd_id: u8, can_id_raw: u32, dlc: u8, data: &[u8]) -> Vec<u8> {
        let mut pkt = vec![0u8; CAND_FRAME_SIZE];
        pkt[0] = cmd_id;
        pkt[8..12].copy_from_slice(&can_id_raw.to_le_bytes());
        pkt[12] = dlc & 0x0F;
        for (i, &b) in data.iter().enumerate().take(8) {
            pkt[16 + i] = b;
        }
        pkt
    }

    /// 解析标准 RX 数据帧
    #[test]
    fn test_parse_rx_frame() {
        let mut engine = CandleEngine::new();
        let pkt = make_rx_frame(CAND_CMD_RX, 0x123, 4, &[0x01, 0x02, 0x03, 0x04]);
        let frames = engine.feed_can(&pkt);
        assert_eq!(frames.len(), 1);
        let f = &frames[0];
        assert_eq!(f.id, 0x123);
        assert!(!f.extended);
        assert!(!f.rtr);
        assert_eq!(f.dlc, 4);
        assert_eq!(f.data, vec![0x01, 0x02, 0x03, 0x04]);
        assert_eq!(f.direction, CanDirection::Rx);
    }

    /// 解析扩展帧 (bit 29 置位)
    #[test]
    fn test_parse_extended_frame() {
        let mut engine = CandleEngine::new();
        let can_id_raw = 0x12345678 | CAND_ID_EFF;
        let pkt = make_rx_frame(
            CAND_CMD_RX,
            can_id_raw,
            8,
            &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08],
        );
        let frames = engine.feed_can(&pkt);
        assert_eq!(frames.len(), 1);
        let f = &frames[0];
        assert_eq!(f.id, 0x12345678);
        assert!(f.extended);
        assert!(!f.rtr);
        assert_eq!(f.dlc, 8);
        assert_eq!(f.data, vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
    }

    /// 解析 RTR 远程帧 (bit 30 置位)
    ///
    /// 注: 按 candle.rs 实现逻辑, 数据字节按 dlc 从包中提取,
    /// RTR 帧的 data 字段长度仍等于 dlc (内容为零), 不强制为空。
    /// 规范中的对应测试只校验 rtr/dlc, 此处与规范保持一致。
    #[test]
    fn test_parse_rtr_frame() {
        let mut engine = CandleEngine::new();
        let can_id_raw = 0x123 | CAND_ID_RTR;
        let pkt = make_rx_frame(CAND_CMD_RX, can_id_raw, 4, &[]);
        let frames = engine.feed_can(&pkt);
        assert_eq!(frames.len(), 1);
        let f = &frames[0];
        assert_eq!(f.id, 0x123);
        assert!(!f.extended);
        assert!(f.rtr);
        assert_eq!(f.dlc, 4);
    }

    /// 分片喂入: 第一次 12 字节, 第二次 12 字节
    #[test]
    fn test_parse_partial() {
        let mut engine = CandleEngine::new();
        let pkt = make_rx_frame(CAND_CMD_RX, 0x123, 4, &[0x01, 0x02, 0x03, 0x04]);
        let frames = engine.feed_can(&pkt[..12]);
        assert!(frames.is_empty());
        let frames = engine.feed_can(&pkt[12..]);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].id, 0x123);
        assert_eq!(frames[0].data, vec![0x01, 0x02, 0x03, 0x04]);
    }

    /// 跳过非帧命令 (cmd_id 不是 0x11/0x12)
    #[test]
    fn test_skip_non_frame_command() {
        let mut engine = CandleEngine::new();
        // 设置波特率响应 (cmd_id = 0x01) — 应被跳过
        let mut pkt = vec![0u8; CAND_FRAME_SIZE];
        pkt[0] = 0x01;
        let frames = engine.feed_can(&pkt);
        assert!(frames.is_empty());

        // 后续接一个有效 RX 帧
        let valid_pkt = make_rx_frame(CAND_CMD_RX, 0x123, 4, &[0x01, 0x02, 0x03, 0x04]);
        let frames = engine.feed_can(&valid_pkt);
        assert_eq!(frames.len(), 1);
    }

    /// 解析 TX 帧 (方向为 Tx)
    #[test]
    fn test_parse_tx_frame() {
        let mut engine = CandleEngine::new();
        let pkt = make_rx_frame(CAND_CMD_TX, 0x123, 2, &[0xAA, 0xBB]);
        let frames = engine.feed_can(&pkt);
        assert_eq!(frames.len(), 1);
        let f = &frames[0];
        assert_eq!(f.direction, CanDirection::Tx);
        assert_eq!(f.data, vec![0xAA, 0xBB]);
    }

    /// 编码 TX 帧
    #[test]
    fn test_encode_tx_frame() {
        let mut engine = CandleEngine::new();
        let frame = CanFrame {
            timestamp: 0,
            id: 0x123,
            extended: false,
            rtr: false,
            dlc: 4,
            data: vec![0x01, 0x02, 0x03, 0x04],
            direction: CanDirection::Tx,
        };
        let encoded = engine.encode_can(&frame);
        assert_eq!(encoded.len(), CAND_FRAME_SIZE);
        assert_eq!(encoded[0], CAND_CMD_TX);
        let can_id_raw = u32::from_le_bytes([encoded[8], encoded[9], encoded[10], encoded[11]]);
        assert_eq!(can_id_raw, 0x123);
        assert_eq!(encoded[12], 4);
        assert_eq!(&encoded[16..20], &[0x01, 0x02, 0x03, 0x04]);
    }

    /// 编码扩展帧 + RTR
    #[test]
    fn test_encode_extended_rtr_frame() {
        let mut engine = CandleEngine::new();
        let frame = CanFrame {
            timestamp: 0,
            id: 0x12345678,
            extended: true,
            rtr: true,
            dlc: 4,
            data: Vec::new(),
            direction: CanDirection::Tx,
        };
        let encoded = engine.encode_can(&frame);
        assert_eq!(encoded.len(), CAND_FRAME_SIZE);
        let can_id_raw = u32::from_le_bytes([encoded[8], encoded[9], encoded[10], encoded[11]]);
        assert_eq!(can_id_raw & CAND_ID_MASK, 0x12345678);
        assert!(can_id_raw & CAND_ID_EFF != 0);
        assert!(can_id_raw & CAND_ID_RTR != 0);
    }

    /// 多帧一次性喂入
    #[test]
    fn test_parse_multiple_frames() {
        let mut engine = CandleEngine::new();
        let mut data = Vec::new();
        data.extend_from_slice(&make_rx_frame(CAND_CMD_RX, 0x100, 1, &[0xAA]));
        data.extend_from_slice(&make_rx_frame(CAND_CMD_RX, 0x200, 1, &[0xBB]));
        let frames = engine.feed_can(&data);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].id, 0x100);
        assert_eq!(frames[1].id, 0x200);
    }

    /// 编码标准数据帧: 验证完整 24 字节包结构 (16 header + 8 data)
    #[test]
    fn test_encode_standard_frame_full_structure() {
        let mut engine = CandleEngine::new();
        let frame = CanFrame {
            timestamp: 0,
            id: 0x123,
            extended: false,
            rtr: false,
            dlc: 4,
            data: vec![0x01, 0x02, 0x03, 0x04],
            direction: CanDirection::Tx,
        };
        let encoded = engine.encode_can(&frame);
        // 24 字节
        assert_eq!(encoded.len(), CAND_FRAME_SIZE);
        // 偏移 0: cmd_id = 0x12 (TX)
        assert_eq!(encoded[0], CAND_CMD_TX);
        // 偏移 1: channel = 0
        assert_eq!(encoded[1], 0);
        // 偏移 2-3: reserved = 0
        assert_eq!(encoded[2], 0);
        assert_eq!(encoded[3], 0);
        // 偏移 4-7: timestamp = 0 (实现未设置)
        assert_eq!(&encoded[4..8], &[0, 0, 0, 0]);
        // 偏移 8-11: CAN ID (LE), 标准 = 0x123, 无 EFF/RTR
        let can_id_raw = u32::from_le_bytes([encoded[8], encoded[9], encoded[10], encoded[11]]);
        assert_eq!(can_id_raw, 0x123);
        assert_eq!(can_id_raw & CAND_ID_EFF, 0); // 标准帧
        assert_eq!(can_id_raw & CAND_ID_RTR, 0); // 数据帧
                                                 // 偏移 12: DLC = 4
        assert_eq!(encoded[12], 4);
        // 偏移 13-15: reserved = 0
        assert_eq!(encoded[13], 0);
        assert_eq!(encoded[14], 0);
        assert_eq!(encoded[15], 0);
        // 偏移 16-23: 数据 (前 4 字节为数据, 后 4 字节为 0)
        assert_eq!(&encoded[16..20], &[0x01, 0x02, 0x03, 0x04]);
        assert_eq!(&encoded[20..24], &[0, 0, 0, 0]);
    }

    /// 编码扩展帧: 验证 CAN ID 的 bit29 (EFF) 标志位设置
    #[test]
    fn test_encode_extended_frame_eff_flag() {
        let mut engine = CandleEngine::new();
        let frame = CanFrame {
            timestamp: 0,
            id: 0x12345678,
            extended: true,
            rtr: false,
            dlc: 8,
            data: vec![0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88],
            direction: CanDirection::Tx,
        };
        let encoded = engine.encode_can(&frame);
        assert_eq!(encoded.len(), CAND_FRAME_SIZE);
        let can_id_raw = u32::from_le_bytes([encoded[8], encoded[9], encoded[10], encoded[11]]);
        assert_eq!(can_id_raw & CAND_ID_MASK, 0x12345678);
        assert!(can_id_raw & CAND_ID_EFF != 0, "EFF 标志位应被设置");
        assert_eq!(can_id_raw & CAND_ID_RTR, 0); // 数据帧, RTR 不应设置
        assert_eq!(encoded[12], 8);
        assert_eq!(
            &encoded[16..24],
            &[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]
        );
    }

    /// 编码远程帧: 验证 bit30 (RTR) 标志位设置
    #[test]
    fn test_encode_rtr_frame_rtr_flag() {
        let mut engine = CandleEngine::new();
        let frame = CanFrame {
            timestamp: 0,
            id: 0x456,
            extended: false,
            rtr: true,
            dlc: 4,
            data: Vec::new(),
            direction: CanDirection::Tx,
        };
        let encoded = engine.encode_can(&frame);
        assert_eq!(encoded.len(), CAND_FRAME_SIZE);
        let can_id_raw = u32::from_le_bytes([encoded[8], encoded[9], encoded[10], encoded[11]]);
        assert_eq!(can_id_raw & CAND_ID_MASK, 0x456);
        assert_eq!(can_id_raw & CAND_ID_EFF, 0); // 标准帧
        assert!(can_id_raw & CAND_ID_RTR != 0, "RTR 标志位应被设置");
        assert_eq!(encoded[12], 4);
        // RTR 帧无数据, 数据区应为 0
        assert_eq!(&encoded[16..24], &[0; 8]);
    }

    /// 编码空数据帧 (dlc=0)
    #[test]
    fn test_encode_empty_data_frame() {
        let mut engine = CandleEngine::new();
        let frame = CanFrame {
            timestamp: 0,
            id: 0x78,
            extended: false,
            rtr: false,
            dlc: 0,
            data: Vec::new(),
            direction: CanDirection::Tx,
        };
        let encoded = engine.encode_can(&frame);
        assert_eq!(encoded.len(), CAND_FRAME_SIZE);
        assert_eq!(encoded[0], CAND_CMD_TX);
        let can_id_raw = u32::from_le_bytes([encoded[8], encoded[9], encoded[10], encoded[11]]);
        assert_eq!(can_id_raw, 0x78);
        assert_eq!(encoded[12], 0);
        // 数据区全为 0
        assert_eq!(&encoded[16..24], &[0; 8]);
    }

    /// Round-trip: 编码标准数据帧后再解析
    #[test]
    fn test_round_trip_standard_data_frame() {
        let mut engine = CandleEngine::new();
        let original = CanFrame {
            timestamp: 0,
            id: 0x123,
            extended: false,
            rtr: false,
            dlc: 4,
            data: vec![0x01, 0x02, 0x03, 0x04],
            direction: CanDirection::Tx,
        };
        let encoded = engine.encode_can(&original);
        let parsed = engine.feed_can(&encoded);
        assert_eq!(parsed.len(), 1);
        let f = &parsed[0];
        assert_eq!(f.id, original.id);
        assert_eq!(f.extended, original.extended);
        assert_eq!(f.rtr, original.rtr);
        assert_eq!(f.dlc, original.dlc);
        assert_eq!(f.data, original.data);
    }

    /// Round-trip: 编码扩展数据帧后再解析
    #[test]
    fn test_round_trip_extended_data_frame() {
        let mut engine = CandleEngine::new();
        let original = CanFrame {
            timestamp: 0,
            id: 0x12345678,
            extended: true,
            rtr: false,
            dlc: 8,
            data: vec![0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88],
            direction: CanDirection::Tx,
        };
        let encoded = engine.encode_can(&original);
        let parsed = engine.feed_can(&encoded);
        assert_eq!(parsed.len(), 1);
        let f = &parsed[0];
        assert_eq!(f.id, original.id);
        assert_eq!(f.extended, original.extended);
        assert_eq!(f.rtr, original.rtr);
        assert_eq!(f.dlc, original.dlc);
        assert_eq!(f.data, original.data);
    }

    /// Round-trip: 编码标准远程帧后再解析
    /// 注: candle 解析 RTR 帧时, data 字段长度 = dlc (内容为零),
    /// 因此只比较 id/extended/rtr/dlc, 不比较 data。
    #[test]
    fn test_round_trip_standard_remote_frame() {
        let mut engine = CandleEngine::new();
        let original = CanFrame {
            timestamp: 0,
            id: 0x456,
            extended: false,
            rtr: true,
            dlc: 4,
            data: Vec::new(),
            direction: CanDirection::Tx,
        };
        let encoded = engine.encode_can(&original);
        let parsed = engine.feed_can(&encoded);
        assert_eq!(parsed.len(), 1);
        let f = &parsed[0];
        assert_eq!(f.id, original.id);
        assert_eq!(f.extended, original.extended);
        assert_eq!(f.rtr, original.rtr);
        assert_eq!(f.dlc, original.dlc);
        // 注: 解析后 data = vec![0; dlc] (4 个零字节), 与原 data (空) 不一致,
        // 这是 candle 实现的预期行为 (按 dlc 从包中提取)。
        assert_eq!(f.data.len(), original.dlc as usize);
    }

    /// Round-trip: 编码扩展远程帧后再解析
    #[test]
    fn test_round_trip_extended_remote_frame() {
        let mut engine = CandleEngine::new();
        let original = CanFrame {
            timestamp: 0,
            id: 0x12345678,
            extended: true,
            rtr: true,
            dlc: 8,
            data: Vec::new(),
            direction: CanDirection::Tx,
        };
        let encoded = engine.encode_can(&original);
        let parsed = engine.feed_can(&encoded);
        assert_eq!(parsed.len(), 1);
        let f = &parsed[0];
        assert_eq!(f.id, original.id);
        assert_eq!(f.extended, original.extended);
        assert_eq!(f.rtr, original.rtr);
        assert_eq!(f.dlc, original.dlc);
        assert_eq!(f.data.len(), original.dlc as usize);
    }

    /// Round-trip: 编码空数据帧后再解析
    #[test]
    fn test_round_trip_empty_data_frame() {
        let mut engine = CandleEngine::new();
        let original = CanFrame {
            timestamp: 0,
            id: 0x78,
            extended: false,
            rtr: false,
            dlc: 0,
            data: Vec::new(),
            direction: CanDirection::Tx,
        };
        let encoded = engine.encode_can(&original);
        let parsed = engine.feed_can(&encoded);
        assert_eq!(parsed.len(), 1);
        let f = &parsed[0];
        assert_eq!(f.id, 0x78);
        assert!(!f.extended);
        assert!(!f.rtr);
        assert_eq!(f.dlc, 0);
        assert!(f.data.is_empty());
    }

    /// Round-trip: 多帧编码后再解析
    #[test]
    fn test_round_trip_multiple_frames() {
        let mut engine = CandleEngine::new();
        let frames = vec![
            CanFrame {
                timestamp: 0,
                id: 0x100,
                extended: false,
                rtr: false,
                dlc: 2,
                data: vec![0xAA, 0xBB],
                direction: CanDirection::Tx,
            },
            CanFrame {
                timestamp: 0,
                id: 0x12345678,
                extended: true,
                rtr: false,
                dlc: 8,
                data: vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08],
                direction: CanDirection::Tx,
            },
        ];
        let mut buf = Vec::new();
        for f in &frames {
            buf.extend(engine.encode_can(f));
        }
        let parsed = engine.feed_can(&buf);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].id, 0x100);
        assert_eq!(parsed[0].data, vec![0xAA, 0xBB]);
        assert_eq!(parsed[1].id, 0x12345678);
        assert!(parsed[1].extended);
        assert_eq!(
            parsed[1].data,
            vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]
        );
    }
}
