use std::fmt::Write as _;
use std::time::{SystemTime, UNIX_EPOCH};

use can_types::{CanDirection, CanFrame};
use protocol_engine::{split_at_boundaries, FeedOutput, ProtocolEngine};

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
    fn now_us() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_micros()).unwrap_or(0))
    }

    /// 解析一行命令, 返回 CAN 帧 (不识别的命令返回 None)
    fn parse_line(line: &[u8]) -> Option<CanFrame> {
        if line.is_empty() {
            return None;
        }
        let cmd = line[0] as char;
        let rest = &line[1..];
        let rest_str = std::str::from_utf8(rest).ok()?;
        match cmd {
            't' | 'T' => Self::parse_data_frame(cmd, rest_str),
            'r' | 'R' => Self::parse_remote_frame(cmd, rest_str),
            // 忽略其他命令 (S/O/C/F/V/N 等) 及错误响应 z\r / \a (BEL)
            _ => None,
        }
    }

    /// 解析数据帧 (t/T 命令)
    fn parse_data_frame(cmd: char, rest: &str) -> Option<CanFrame> {
        let extended = cmd == 'T';
        let id_len = if extended { 8 } else { 3 };
        if rest.len() < id_len + 1 {
            return None;
        }
        let id = u32::from_str_radix(&rest[..id_len], 16).ok()?;
        let dlc_char = rest.as_bytes()[id_len] as char;
        let dlc = u8::try_from(dlc_char.to_digit(16)?).ok()?;
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
            timestamp: Self::now_us(),
            id,
            extended,
            rtr: false,
            dlc,
            data,
            direction: CanDirection::Rx,
        })
    }

    /// 解析远程帧 (r/R 命令, 无数据部分)
    fn parse_remote_frame(cmd: char, rest: &str) -> Option<CanFrame> {
        let extended = cmd == 'R';
        let id_len = if extended { 8 } else { 3 };
        if rest.len() < id_len + 1 {
            return None;
        }
        let id = u32::from_str_radix(&rest[..id_len], 16).ok()?;
        let dlc_char = rest.as_bytes()[id_len] as char;
        let dlc = u8::try_from(dlc_char.to_digit(16)?).ok()?;
        if dlc > 8 {
            return None;
        }
        Some(CanFrame {
            timestamp: Self::now_us(),
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
    fn feed(&mut self, data: &[u8]) -> FeedOutput {
        // slcan 不产生 DataFrame, 只产生 CanFrame
        self.line_buf.extend_from_slice(data);
        let mut frames = Vec::new();
        // 单趟扫描: 循环内只记录行边界, 结束后一次性 drain
        // (原实现逐行 drain(..=pos).collect(), 大批次下 O(n²) memmove)
        let mut line_start = 0usize;
        for i in 0..self.line_buf.len() {
            let b = self.line_buf[i];
            if b == b'\r' || b == b'\n' {
                let line = &self.line_buf[line_start..i];
                if !line.is_empty() {
                    if let Some(frame) = Self::parse_line(line) {
                        frames.push(frame);
                    }
                }
                line_start = i + 1;
            }
        }
        if line_start > 0 {
            self.line_buf.drain(..line_start);
        }
        // 缓冲区溢出保护: 超过 4096 字节时丢弃前半部分
        if self.line_buf.len() > 4096 {
            let drop = self.line_buf.len() - 2048;
            self.line_buf.drain(..drop);
        }
        FeedOutput::from_can_frames(frames)
    }

    fn encode_can(&mut self, frame: &CanFrame) -> Vec<u8> {
        let mut s = String::with_capacity(32);
        if frame.rtr {
            // 远程帧用 r/R 命令 (无 data 部分)
            if frame.extended {
                s.push('R');
                let _ = write!(s, "{:08X}", frame.id);
            } else {
                s.push('r');
                let _ = write!(s, "{:03X}", frame.id);
            }
            let _ = write!(s, "{:X}", frame.dlc);
        } else {
            // 数据帧用 t/T 命令
            if frame.extended {
                s.push('T');
                let _ = write!(s, "{:08X}", frame.id);
            } else {
                s.push('t');
                let _ = write!(s, "{:03X}", frame.id);
            }
            let _ = write!(s, "{:X}", frame.dlc);
            for &b in &frame.data {
                let _ = write!(s, "{b:02X}");
            }
        }
        s.push('\r');
        s.into_bytes()
    }

    fn encode_channel(&mut self, _channel: usize, value: f32) -> Vec<u8> {
        // slcan 引擎不直接编码通道值, 保留 FireWater 风格作为兼容
        format!("{value:.6}\n").into_bytes()
    }

    fn encode_channels(&mut self, values: &[f32]) -> Vec<u8> {
        let s: Vec<String> = values.iter().map(|v| format!("{v:.6}")).collect();
        format!("{}\n", s.join(",")).into_bytes()
    }

    fn name(&self) -> &'static str {
        "Slcan"
    }

    fn encode_frame(&mut self, _frame: &vofa_core::DataFrame) -> Vec<u8> {
        // CAN 协议无法从 DataFrame 重编码 (需走 encode_can), 语义不符
        Vec::new()
    }

    fn split_aligned(&self, data: &[u8], workers: usize) -> Option<Vec<std::ops::Range<usize>>> {
        // 边界 = 每个 \r / \n 之后的位置 (与 feed 的行扫描逻辑一致)
        let boundaries: Vec<usize> = data
            .iter()
            .enumerate()
            .filter(|(_, &b)| b == b'\r' || b == b'\n')
            .map(|(i, _)| i + 1)
            .collect();
        Some(split_at_boundaries(&boundaries, workers))
    }

    fn take_pending(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.line_buf)
    }

    fn new_worker(&self) -> Box<dyn ProtocolEngine> {
        Box::new(Self::new())
    }
}

impl Default for SlcanEngine {
    fn default() -> Self {
        Self::new()
    }
}