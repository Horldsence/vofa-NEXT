//! 协议引擎核心 — ProtocolEngine trait 与跨协议统一的输入/输出容器
//!
//! 子模块:
//! - [`parse`]: 输入字符串解析自由函数 (parse_hex / parse_ascii / detect_format)
//! - [`split`]: 帧边界并行切分算法 (split_at_boundaries)

pub mod parse;
pub mod split;

use serde::{Deserialize, Serialize};
use vofa_next_core::{CanFrame, DataFrame, DecodedEvent, LogicSample};

pub use parse::{detect_format, parse_ascii, parse_hex};
pub use split::split_at_boundaries;

/// 输入解析格式 — 控制前端传入的字符串如何转为字节
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InputFormat {
    /// HEX 字节流 ("AA 01 02" / "AA0102" / "AA,01,02" 均可)
    Hex,
    /// ASCII 文本 + 转义 (\n \r \t \xHH \0 \\)
    Ascii,
    /// 自动识别 — 启发式判断 HEX 或 ASCII
    ///
    /// 规则: 去除 "0x" 前缀 / 空白 / 逗号后, 若剩余字符全部为十六进制 (0-9a-fA-F)
    /// 且长度为偶数, 则视为 HEX; 否则视为 ASCII。
    /// 例: "AA 01 02" → HEX; "1.0,2.0\n" → ASCII; "Hello" → ASCII
    Auto,
}

/// 协议解析结果 — 由 parse_input 返回, 跨协议统一容器
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum ParsedInput {
    /// DataFrame 列表 (JustFloat / FireWater)
    Frames(Vec<DataFrame>),
    /// CAN 帧列表 (Slcan / CandleLight)
    CanFrames(Vec<CanFrame>),
    /// 逻辑采样 (LogicDecode)
    LogicSamples(Vec<LogicSample>),
    /// 解码事件 (LogicDecode)
    DecodedEvents(Vec<DecodedEvent>),
    /// 原始字节预览 (RawData / 无法结构化解析的协议)
    RawBytes(Vec<u8>),
    /// 解析错误
    Error { message: String },
}

impl ParsedInput {
    pub fn error(msg: impl Into<String>) -> Self {
        Self::Error {
            message: msg.into(),
        }
    }
}

/// 喂入结果 — 单次 feed 的全量输出, 各引擎只填自己支持的字段
#[derive(Debug, Default)]
pub struct FeedOutput {
    /// 数据帧 (JustFloat / FireWater)
    pub frames: Vec<DataFrame>,
    /// CAN 帧 (Slcan / CandleLight)
    pub can_frames: Vec<CanFrame>,
    /// 逻辑采样 (LogicDecoder)
    pub logic_samples: Vec<LogicSample>,
    /// 协议解码事件 (LogicDecoder)
    pub decoded_events: Vec<DecodedEvent>,
}

impl FeedOutput {
    /// 仅含数据帧
    pub fn from_frames(frames: Vec<DataFrame>) -> Self {
        Self {
            frames,
            ..Default::default()
        }
    }
    /// 仅含 CAN 帧
    pub fn from_can_frames(can_frames: Vec<CanFrame>) -> Self {
        Self {
            can_frames,
            ..Default::default()
        }
    }
    /// 按序合并另一份输出 (并行 worker 结果按块序逐个 append)
    pub fn append(&mut self, other: FeedOutput) {
        self.frames.extend(other.frames);
        self.can_frames.extend(other.can_frames);
        self.logic_samples.extend(other.logic_samples);
        self.decoded_events.extend(other.decoded_events);
    }
}

/// 协议引擎 trait — 解析接收数据 / 编码发送数据
pub trait ProtocolEngine: Send {
    /// 喂入原始字节流, 单趟解析并返回该协议支持的全部输出
    fn feed(&mut self, data: &[u8]) -> FeedOutput;

    /// 编码单通道值为字节流 (用于自动绑定模式发送)
    fn encode_channel(&mut self, channel: usize, value: f32) -> Vec<u8>;

    /// 编码多通道值 (一次性发送所有通道)
    fn encode_channels(&mut self, values: &[f32]) -> Vec<u8>;

    /// 协议名称
    fn name(&self) -> &str;

    /// 自动检测到的通道数 (自动模式下, 收到首帧后返回 Some(n))
    /// 手动模式或未检测到时返回 None
    fn detected_channels(&self) -> Option<usize> {
        None
    }

    /// 是否为自动检测模式
    fn is_auto_mode(&self) -> bool {
        false
    }

    /// 通用重编码入口: 把解析出的数据帧按本协议编码回字节流 (协议转换用)。
    /// 默认实现按通道值编码; 自动通道数模式下以输入帧通道数为准。
    fn encode_frame(&mut self, frame: &DataFrame) -> Vec<u8> {
        self.encode_channels(&frame.channels)
    }

    /// 编码 CAN 帧为传输字节 (仅 Slcan/CandleLight 引擎重写)
    fn encode_can(&mut self, _frame: &CanFrame) -> Vec<u8> {
        Vec::new()
    }

    /// 帧对齐切分: 把 data 的完整帧前缀均分为 workers 块 (每块以帧边界结尾, 升序 Range),
    /// 尾部不完整帧 (最后一个 Range 结束位置之后) 由调用方保留拼接。
    /// 返回 None = 协议跨帧有状态或无帧概念, 不支持并行解析 (LogicDecoder / RawData)。
    fn split_aligned(&self, _data: &[u8], _workers: usize) -> Option<Vec<std::ops::Range<usize>>> {
        None
    }

    /// 取出内部缓冲的未解析字节 (顺序 → 并行模式切换时调用)
    fn take_pending(&mut self) -> Vec<u8> {
        Vec::new()
    }

    /// 新建一个同配置、空状态的引擎 (并行 worker 用)
    fn new_worker(&self) -> Box<dyn ProtocolEngine>;

    /// 解析用户输入字符串为协议帧 (用于输入协议分析 / 协议解码器面板)
    ///
    /// - `input`: 用户输入的原始字符串
    /// - `format`: 输入格式 (HEX 或 ASCII)
    ///
    /// 默认实现: 将 input 按 format 转为字节, 然后调用 feed 单次解析,
    /// 收集所有可解析出的结果。各协议引擎可重写以提供更精确的解析。
    fn parse_input(&mut self, input: &str, format: InputFormat) -> ParsedInput {
        let resolved = match format {
            InputFormat::Auto => detect_format(input),
            other => other,
        };
        let bytes = match resolved {
            InputFormat::Hex => match parse_hex(input) {
                Ok(b) => b,
                Err(e) => return ParsedInput::error(e),
            },
            InputFormat::Ascii => parse_ascii(input),
            InputFormat::Auto => unreachable!("detect_format never returns Auto"),
        };
        if bytes.is_empty() {
            return ParsedInput::error("输入为空");
        }
        // 默认行为: 单次 feed 全量解析, 按优先级返回; 若无结果则返回 RawBytes
        let out = self.feed(&bytes);
        if !out.frames.is_empty() {
            return ParsedInput::Frames(out.frames);
        }
        if !out.can_frames.is_empty() {
            return ParsedInput::CanFrames(out.can_frames);
        }
        if !out.logic_samples.is_empty() {
            return ParsedInput::LogicSamples(out.logic_samples);
        }
        if !out.decoded_events.is_empty() {
            return ParsedInput::DecodedEvents(out.decoded_events);
        }
        ParsedInput::RawBytes(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CandleEngine, FireWaterEngine, JustFloatEngine, RawDataEngine, SlcanEngine};
    use vofa_next_core::CanDirection;

    #[test]
    fn test_parse_input_auto_resolves_hex() {
        let mut engine = RawDataEngine::new();
        // "AA 01 02 BB" 应自动识别为 HEX
        let result = engine.parse_input("AA 01 02 BB", InputFormat::Auto);
        match result {
            ParsedInput::RawBytes(bytes) => {
                assert_eq!(bytes, vec![0xAA, 0x01, 0x02, 0xBB]);
            }
            other => panic!("expected RawBytes, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_input_auto_resolves_ascii() {
        let mut engine = FireWaterEngine::new(Some(3));
        // "1.0,2.0,3.0\\n" 含 '.' 与 '\\' → 自动识别为 ASCII
        let result = engine.parse_input("1.0,2.0,3.0\\n", InputFormat::Auto);
        match result {
            ParsedInput::Frames(frames) => {
                assert_eq!(frames.len(), 1);
                assert_eq!(frames[0].channels, vec![1.0, 2.0, 3.0]);
            }
            other => panic!("expected Frames, got {:?}", other),
        }
    }

    // ===== parse_input 跨协议测试 =====

    #[test]
    fn test_parse_input_justfloat_hex() {
        // 1.0 (LE) + 2.0 (LE) + tail 00 00 80 7F
        let mut engine = JustFloatEngine::new(None);
        let input = "0000803F 00000040 0000807F";
        let result = engine.parse_input(input, InputFormat::Hex);
        match result {
            ParsedInput::Frames(frames) => {
                assert_eq!(frames.len(), 1);
                assert_eq!(frames[0].channels, vec![1.0, 2.0]);
            }
            other => panic!("expected Frames, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_input_firewater_ascii() {
        let mut engine = FireWaterEngine::new(Some(3));
        let input = "1.0,2.0,3.0\\n";
        let result = engine.parse_input(input, InputFormat::Ascii);
        match result {
            ParsedInput::Frames(frames) => {
                assert_eq!(frames.len(), 1);
                assert_eq!(frames[0].channels, vec![1.0, 2.0, 3.0]);
            }
            other => panic!("expected Frames, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_input_slcan_ascii() {
        let mut engine = SlcanEngine::new();
        let input = "t123401020304\\r";
        let result = engine.parse_input(input, InputFormat::Ascii);
        match result {
            ParsedInput::CanFrames(frames) => {
                assert_eq!(frames.len(), 1);
                assert_eq!(frames[0].id, 0x123);
                assert_eq!(frames[0].dlc, 4);
                assert_eq!(frames[0].data, vec![0x01, 0x02, 0x03, 0x04]);
                assert_eq!(frames[0].direction, CanDirection::Rx);
            }
            other => panic!("expected CanFrames, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_input_candle_hex() {
        // 构造 24 字节 RX 帧: cmd=0x11, id=0x123 (LE), dlc=4, data=01 02 03 04
        let mut engine = CandleEngine::new();
        let input = "11 00 00 00 00 00 00 00 23 01 00 00 04 00 00 00 01 02 03 04 00 00 00 00";
        let result = engine.parse_input(input, InputFormat::Hex);
        match result {
            ParsedInput::CanFrames(frames) => {
                assert_eq!(frames.len(), 1);
                assert_eq!(frames[0].id, 0x123);
                assert_eq!(frames[0].dlc, 4);
                assert_eq!(frames[0].data, vec![0x01, 0x02, 0x03, 0x04]);
            }
            other => panic!("expected CanFrames, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_input_rawdata_returns_raw_bytes() {
        let mut engine = RawDataEngine::new();
        let input = "AA 01 02 BB";
        let result = engine.parse_input(input, InputFormat::Hex);
        match result {
            ParsedInput::RawBytes(bytes) => {
                assert_eq!(bytes, vec![0xAA, 0x01, 0x02, 0xBB]);
            }
            other => panic!("expected RawBytes, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_input_empty_returns_error() {
        let mut engine = RawDataEngine::new();
        let result = engine.parse_input("", InputFormat::Hex);
        match result {
            ParsedInput::Error { message } => assert!(message.contains("空")),
            other => panic!("expected Error, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_input_invalid_hex_returns_error() {
        let mut engine = RawDataEngine::new();
        let result = engine.parse_input("ZZ", InputFormat::Hex);
        match result {
            ParsedInput::Error { .. } => {}
            other => panic!("expected Error, got {:?}", other),
        }
    }
}
