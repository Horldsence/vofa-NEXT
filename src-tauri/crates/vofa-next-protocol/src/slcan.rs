use std::time::{SystemTime, UNIX_EPOCH};
use vofa_next_core::{CanDirection, CanFrame, DataFrame};

use crate::engine::ProtocolEngine;

/// slcan (Lawicel ASCII) 协议引擎
///
/// 命令以 `\r` (0x0D) 结尾, 部分实现也接受 `\n` (0x0A)
///
/// 接收帧命令:
/// - `t<id><dlc><data>\r` — 标准帧, id 为 3 位十六进制, dlc 为 1 位十六进制, data 为 dlc*2 位十六进制
/// - `T<id><dlc><data>\r` — 扩展帧, id 为 8 位十六进制
/// - `r<id><dlc>\r` — 标准远程帧 (无数据)
/// - `R<id><dlc>\r` — 扩展远程帧 (无数据)
///
/// 其他命令 (S#/O/C/F/V/N 等) 忽略, 不产生 CanFrame
pub struct SlcanEngine {
    /// 行缓冲, 按 `\r` 或 `\n` 分割
    line_buf: Vec<u8>,
}

impl SlcanEngine {
    pub fn new() -> Self {
        Self {
            line_buf: Vec::with_capacity(256),
        }
    }

    /// 当前系统时间 (微秒, 与 DataFrame::new 一致)
    fn now_us(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0)
    }

    /// 解析一行命令, 返回 CAN 帧 (不识别的命令返回 None)
    fn parse_line(&self, line: &[u8]) -> Option<CanFrame> {
        if line.is_empty() {
            return None;
        }
        let cmd = line[0] as char;
        let rest = &line[1..];
        let rest_str = std::str::from_utf8(rest).ok()?;
        match cmd {
            't' | 'T' => self.parse_data_frame(cmd, rest_str),
            'r' | 'R' => self.parse_remote_frame(cmd, rest_str),
            // 忽略其他命令 (S/O/C/F/V/N 等) 及错误响应 z\r / \a (BEL)
            _ => None,
        }
    }

    /// 解析数据帧 (t/T 命令)
    fn parse_data_frame(&self, cmd: char, rest: &str) -> Option<CanFrame> {
        let extended = cmd == 'T';
        let id_len = if extended { 8 } else { 3 };
        if rest.len() < id_len + 1 {
            return None;
        }
        let id = u32::from_str_radix(&rest[..id_len], 16).ok()?;
        let dlc_char = rest.as_bytes()[id_len] as char;
        let dlc = dlc_char.to_digit(16)? as u8;
        if dlc > 8 {
            return None;
        }
        let data_hex = &rest[id_len + 1..];
        if data_hex.len() < dlc as usize * 2 {
            return None;
        }
        let mut data = Vec::with_capacity(dlc as usize);
        for i in 0..dlc as usize {
            let byte = u8::from_str_radix(&data_hex[i * 2..i * 2 + 2], 16).ok()?;
            data.push(byte);
        }
        Some(CanFrame {
            timestamp: self.now_us(),
            id,
            extended,
            rtr: false,
            dlc,
            data,
            direction: CanDirection::Rx,
        })
    }

    /// 解析远程帧 (r/R 命令, 无数据部分)
    fn parse_remote_frame(&self, cmd: char, rest: &str) -> Option<CanFrame> {
        let extended = cmd == 'R';
        let id_len = if extended { 8 } else { 3 };
        if rest.len() < id_len + 1 {
            return None;
        }
        let id = u32::from_str_radix(&rest[..id_len], 16).ok()?;
        let dlc_char = rest.as_bytes()[id_len] as char;
        let dlc = dlc_char.to_digit(16)? as u8;
        if dlc > 8 {
            return None;
        }
        Some(CanFrame {
            timestamp: self.now_us(),
            id,
            extended,
            rtr: true,
            dlc,
            data: Vec::new(),
            direction: CanDirection::Rx,
        })
    }
}

impl ProtocolEngine for SlcanEngine {
    fn feed(&mut self, _data: &[u8]) -> Vec<DataFrame> {
        // slcan 不产生 DataFrame, 只产生 CanFrame (通过 feed_can)
        Vec::new()
    }

    fn feed_can(&mut self, data: &[u8]) -> Vec<CanFrame> {
        self.line_buf.extend_from_slice(data);
        let mut frames = Vec::new();
        loop {
            // 找到行结束符 (\r 或 \n)
            let pos = self.line_buf.iter().position(|&b| b == b'\r' || b == b'\n');
            if pos.is_none() {
                break;
            }
            let pos = pos.unwrap();
            let line: Vec<u8> = self.line_buf.drain(..=pos).collect();
            // 去掉末尾的 \r 或 \n
            let line = &line[..line.len().saturating_sub(1)];
            if !line.is_empty() {
                if let Some(frame) = self.parse_line(line) {
                    frames.push(frame);
                }
            }
        }
        // 缓冲区溢出保护: 超过 4096 字节时丢弃前半部分
        if self.line_buf.len() > 4096 {
            let drop = self.line_buf.len() - 2048;
            self.line_buf.drain(..drop);
        }
        frames
    }

    fn encode_can(&mut self, frame: &CanFrame) -> Vec<u8> {
        let mut s = String::with_capacity(32);
        if frame.rtr {
            // 远程帧用 r/R 命令 (无 data 部分)
            if frame.extended {
                s.push('R');
                s.push_str(&format!("{:08X}", frame.id));
            } else {
                s.push('r');
                s.push_str(&format!("{:03X}", frame.id));
            }
            s.push_str(&format!("{:X}", frame.dlc));
        } else {
            // 数据帧用 t/T 命令
            if frame.extended {
                s.push('T');
                s.push_str(&format!("{:08X}", frame.id));
            } else {
                s.push('t');
                s.push_str(&format!("{:03X}", frame.id));
            }
            s.push_str(&format!("{:X}", frame.dlc));
            for &b in &frame.data {
                s.push_str(&format!("{:02X}", b));
            }
        }
        s.push('\r');
        s.into_bytes()
    }

    fn encode_channel(&mut self, _channel: usize, value: f32) -> Vec<u8> {
        // slcan 引擎不直接编码通道值, 保留 FireWater 风格作为兼容
        format!("{:.6}\n", value).into_bytes()
    }

    fn encode_channels(&mut self, values: &[f32]) -> Vec<u8> {
        let s: Vec<String> = values.iter().map(|v| format!("{:.6}", v)).collect();
        format!("{}\n", s.join(",")).into_bytes()
    }

    fn name(&self) -> &str {
        "Slcan"
    }
}

impl Default for SlcanEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 解析标准数据帧: t123401020304\r -> id=0x123, dlc=4, data=[0x01,0x02,0x03,0x04]
    #[test]
    fn test_parse_standard_frame() {
        let mut engine = SlcanEngine::new();
        let frames = engine.feed_can(b"t123401020304\r");
        assert_eq!(frames.len(), 1);
        let f = &frames[0];
        assert_eq!(f.id, 0x123);
        assert!(!f.extended);
        assert!(!f.rtr);
        assert_eq!(f.dlc, 4);
        assert_eq!(f.data, vec![0x01, 0x02, 0x03, 0x04]);
        assert_eq!(f.direction, CanDirection::Rx);
    }

    /// 解析扩展数据帧: T1234567880102030405060708\r
    #[test]
    fn test_parse_extended_frame() {
        let mut engine = SlcanEngine::new();
        let frames = engine.feed_can(b"T1234567880102030405060708\r");
        assert_eq!(frames.len(), 1);
        let f = &frames[0];
        assert_eq!(f.id, 0x12345678);
        assert!(f.extended);
        assert!(!f.rtr);
        assert_eq!(f.dlc, 8);
        assert_eq!(f.data, vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
    }

    /// 解析远程帧: r1234\r
    #[test]
    fn test_parse_remote_frame() {
        let mut engine = SlcanEngine::new();
        let frames = engine.feed_can(b"r1234\r");
        assert_eq!(frames.len(), 1);
        let f = &frames[0];
        assert_eq!(f.id, 0x123);
        assert!(!f.extended);
        assert!(f.rtr);
        assert_eq!(f.dlc, 4);
        assert!(f.data.is_empty());

        // 扩展远程帧
        let frames = engine.feed_can(b"R123456784\r");
        assert_eq!(frames.len(), 1);
        let f = &frames[0];
        assert_eq!(f.id, 0x12345678);
        assert!(f.extended);
        assert!(f.rtr);
        assert_eq!(f.dlc, 4);
    }

    /// 分片喂入: 第一次 t1234 不完整, 第二次补齐
    #[test]
    fn test_parse_partial() {
        let mut engine = SlcanEngine::new();
        let frames = engine.feed_can(b"t1234");
        assert!(frames.is_empty());
        let frames = engine.feed_can(b"01020304\r");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].id, 0x123);
        assert_eq!(frames[0].data, vec![0x01, 0x02, 0x03, 0x04]);
    }

    /// 忽略非帧命令 (S/O/C/F/V/N 等) 和错误响应
    #[test]
    fn test_ignore_other_commands() {
        let mut engine = SlcanEngine::new();
        // 设置波特率 + 打开 + 版本 + 序列号 + 错误响应, 均不应产生帧
        let frames = engine.feed_can(b"S6\rO\rV\rN1234\rz\r");
        assert!(frames.is_empty());

        // 混合: 命令 + 数据帧
        let frames = engine.feed_can(b"S6\rt123401020304\r");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].id, 0x123);
    }

    /// 接受 \n 作为行结束符
    #[test]
    fn test_accept_newline_terminator() {
        let mut engine = SlcanEngine::new();
        let frames = engine.feed_can(b"t123401020304\n");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].id, 0x123);
    }

    /// 编码标准数据帧
    #[test]
    fn test_encode_standard_frame() {
        let mut engine = SlcanEngine::new();
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
        assert_eq!(encoded, b"t123401020304\r");
    }

    /// 编码扩展数据帧
    #[test]
    fn test_encode_extended_frame() {
        let mut engine = SlcanEngine::new();
        let frame = CanFrame {
            timestamp: 0,
            id: 0x12345678,
            extended: true,
            rtr: false,
            dlc: 8,
            data: vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08],
            direction: CanDirection::Tx,
        };
        let encoded = engine.encode_can(&frame);
        assert_eq!(encoded, b"T1234567880102030405060708\r");
    }

    /// 编码远程帧
    #[test]
    fn test_encode_remote_frame() {
        let mut engine = SlcanEngine::new();
        let frame = CanFrame {
            timestamp: 0,
            id: 0x123,
            extended: false,
            rtr: true,
            dlc: 4,
            data: Vec::new(),
            direction: CanDirection::Tx,
        };
        let encoded = engine.encode_can(&frame);
        assert_eq!(encoded, b"r1234\r");

        let frame_ext = CanFrame {
            timestamp: 0,
            id: 0x12345678,
            extended: true,
            rtr: true,
            dlc: 4,
            data: Vec::new(),
            direction: CanDirection::Tx,
        };
        let encoded = engine.encode_can(&frame_ext);
        assert_eq!(encoded, b"R123456784\r");
    }

    /// 多帧一次性喂入
    #[test]
    fn test_parse_multiple_frames() {
        let mut engine = SlcanEngine::new();
        let frames = engine.feed_can(b"t123401020304\rt123401020304\r");
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].id, 0x123);
        assert_eq!(frames[1].id, 0x123);
    }

    /// DLC 为 0 的数据帧
    #[test]
    fn test_parse_zero_dlc() {
        let mut engine = SlcanEngine::new();
        let frames = engine.feed_can(b"t1230\r");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].dlc, 0);
        assert!(frames[0].data.is_empty());
    }

    /// 缓冲区溢出保护: 喂入大量无终止符的数据不应崩溃
    #[test]
    fn test_buffer_overflow_protection() {
        let mut engine = SlcanEngine::new();
        // 喂入 8000 字节无 \r 的数据
        let junk = vec![b'x'; 8000];
        let frames = engine.feed_can(&junk);
        assert!(frames.is_empty());
        // 缓冲区应被截断到 2048 字节
        assert!(engine.line_buf.len() <= 4096);
    }

    /// 编码标准数据帧 (2 字节): id=0x123, data=[0xAA, 0xBB] → "t1232AABB\r"
    #[test]
    fn test_encode_standard_frame_2bytes() {
        let mut engine = SlcanEngine::new();
        let frame = CanFrame {
            timestamp: 0,
            id: 0x123,
            extended: false,
            rtr: false,
            dlc: 2,
            data: vec![0xAA, 0xBB],
            direction: CanDirection::Tx,
        };
        let encoded = engine.encode_can(&frame);
        assert_eq!(encoded, b"t1232AABB\r");
    }

    /// 编码标准远程帧: id=0x100, dlc=0 → "r1000\r"
    #[test]
    fn test_encode_standard_remote_id_100() {
        let mut engine = SlcanEngine::new();
        let frame = CanFrame {
            timestamp: 0,
            id: 0x100,
            extended: false,
            rtr: true,
            dlc: 0,
            data: Vec::new(),
            direction: CanDirection::Tx,
        };
        let encoded = engine.encode_can(&frame);
        assert_eq!(encoded, b"r1000\r");
    }

    /// 编码空数据帧 (dlc=0): id=0x123 → "t1230\r"
    #[test]
    fn test_encode_empty_data_frame() {
        let mut engine = SlcanEngine::new();
        let frame = CanFrame {
            timestamp: 0,
            id: 0x123,
            extended: false,
            rtr: false,
            dlc: 0,
            data: Vec::new(),
            direction: CanDirection::Tx,
        };
        let encoded = engine.encode_can(&frame);
        assert_eq!(encoded, b"t1230\r");
    }

    /// 编码扩展远程帧: id=0x12345678, dlc=0 → "R123456780\r"
    #[test]
    fn test_encode_extended_remote_frame_standalone() {
        let mut engine = SlcanEngine::new();
        let frame = CanFrame {
            timestamp: 0,
            id: 0x12345678,
            extended: true,
            rtr: true,
            dlc: 0,
            data: Vec::new(),
            direction: CanDirection::Tx,
        };
        let encoded = engine.encode_can(&frame);
        assert_eq!(encoded, b"R123456780\r");
    }

    /// 编码 8 字节数据帧 (验证最大数据长度)
    #[test]
    fn test_encode_max_data_frame() {
        let mut engine = SlcanEngine::new();
        let frame = CanFrame {
            timestamp: 0,
            id: 0x7FF,
            extended: false,
            rtr: false,
            dlc: 8,
            data: vec![0xFF; 8],
            direction: CanDirection::Tx,
        };
        let encoded = engine.encode_can(&frame);
        assert_eq!(encoded, b"t7FF8FFFFFFFFFFFFFFFF\r");
    }

    /// Round-trip: 标准数据帧 编码后再解析
    #[test]
    fn test_round_trip_standard_data_frame() {
        let mut engine = SlcanEngine::new();
        let original = CanFrame {
            timestamp: 0,
            id: 0x123,
            extended: false,
            rtr: false,
            dlc: 2,
            data: vec![0xAA, 0xBB],
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

    /// Round-trip: 扩展数据帧 编码后再解析
    #[test]
    fn test_round_trip_extended_data_frame() {
        let mut engine = SlcanEngine::new();
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

    /// Round-trip: 标准远程帧 编码后再解析
    #[test]
    fn test_round_trip_standard_remote_frame() {
        let mut engine = SlcanEngine::new();
        let original = CanFrame {
            timestamp: 0,
            id: 0x100,
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
        // 远程帧解析后 data 为空
        assert!(f.data.is_empty());
    }

    /// Round-trip: 扩展远程帧 编码后再解析
    #[test]
    fn test_round_trip_extended_remote_frame() {
        let mut engine = SlcanEngine::new();
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
        assert!(f.data.is_empty());
    }

    /// Round-trip: 空数据帧 编码后再解析
    #[test]
    fn test_round_trip_empty_data_frame() {
        let mut engine = SlcanEngine::new();
        let original = CanFrame {
            timestamp: 0,
            id: 0x55,
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
        assert_eq!(f.id, 0x55);
        assert!(!f.extended);
        assert!(!f.rtr);
        assert_eq!(f.dlc, 0);
        assert!(f.data.is_empty());
    }

    /// Round-trip: 多帧编码后再解析
    #[test]
    fn test_round_trip_multiple_frames() {
        let mut engine = SlcanEngine::new();
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
                id: 0x7FF,
                extended: false,
                rtr: true,
                dlc: 8,
                data: Vec::new(),
                direction: CanDirection::Tx,
            },
        ];
        // 编码两帧后一次性喂入
        let mut buf = Vec::new();
        for f in &frames {
            buf.extend(engine.encode_can(f));
        }
        let parsed = engine.feed_can(&buf);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].id, 0x100);
        assert_eq!(parsed[0].data, vec![0xAA, 0xBB]);
        assert_eq!(parsed[1].id, 0x7FF);
        assert!(parsed[1].rtr);
    }
}
