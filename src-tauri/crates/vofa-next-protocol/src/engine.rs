use serde::{Deserialize, Serialize};
use vofa_next_core::{CanFrame, DataFrame, DecodedEvent, LogicSample};

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

/// 自动识别输入格式 — 返回应该使用的具体格式 (Hex 或 Ascii)
///
/// 规则: 去除 "0x" / 空白 / 逗号后, 全为十六进制字符且长度为偶数 → Hex, 否则 Ascii
pub fn detect_format(input: &str) -> InputFormat {
    let no_prefix = input.replace("0x", "").replace("0X", "");
    let clean: String = no_prefix
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',')
        .collect();
    if clean.is_empty() {
        return InputFormat::Ascii;
    }
    if clean.len() % 2 != 0 {
        return InputFormat::Ascii;
    }
    if clean.chars().all(|c| c.is_ascii_hexdigit()) {
        InputFormat::Hex
    } else {
        InputFormat::Ascii
    }
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
        Self::Error { message: msg.into() }
    }
}

/// 协议引擎 trait — 解析接收数据 / 编码发送数据
pub trait ProtocolEngine: Send {
    /// 喂入原始字节流, 返回解析出的数据帧列表
    fn feed(&mut self, data: &[u8]) -> Vec<DataFrame>;

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

    /// 解析 CAN 帧 (仅 Slcan/CandleLight 引擎重写)
    fn feed_can(&mut self, _data: &[u8]) -> Vec<CanFrame> {
        Vec::new()
    }

    /// 编码 CAN 帧为传输字节 (仅 Slcan/CandleLight 引擎重写)
    fn encode_can(&mut self, _frame: &CanFrame) -> Vec<u8> {
        Vec::new()
    }

    /// 解析逻辑分析仪采样 (仅 LogicDecoder 引擎重写)
    fn feed_logic(&mut self, _data: &[u8]) -> Vec<LogicSample> {
        Vec::new()
    }

    /// 解析协议解码事件 (仅 LogicDecoder 引擎重写)
    /// 输入原始字节流, 输出 UART/I2C/SPI 解码事件
    fn feed_decoded(&mut self, _data: &[u8]) -> Vec<DecodedEvent> {
        Vec::new()
    }

    /// 解析用户输入字符串为协议帧 (用于输入协议分析 / 协议解码器面板)
    ///
    /// - `input`: 用户输入的原始字符串
    /// - `format`: 输入格式 (HEX 或 ASCII)
    ///
    /// 默认实现: 将 input 按 format 转为字节, 然后调用 feed / feed_can / feed_logic /
    /// feed_decoded, 收集所有可解析出的结果。各协议引擎可重写以提供更精确的解析。
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
        // 默认行为: 尝试 feed, 若无结果则返回 RawBytes
        let frames = self.feed(&bytes);
        if !frames.is_empty() {
            return ParsedInput::Frames(frames);
        }
        let can_frames = self.feed_can(&bytes);
        if !can_frames.is_empty() {
            return ParsedInput::CanFrames(can_frames);
        }
        let logic = self.feed_logic(&bytes);
        if !logic.is_empty() {
            return ParsedInput::LogicSamples(logic);
        }
        let decoded = self.feed_decoded(&bytes);
        if !decoded.is_empty() {
            return ParsedInput::DecodedEvents(decoded);
        }
        ParsedInput::RawBytes(bytes)
    }
}

/// 解析 HEX 字符串为字节 — 兼容 "AA 01" / "AA01" / "AA,01" / "0xAA 0x01"
pub fn parse_hex(input: &str) -> Result<Vec<u8>, String> {
    // 先去掉所有 "0x" / "0X" 前缀, 再过滤空白与逗号
    let no_prefix = input.replace("0x", "").replace("0X", "");
    let clean: String = no_prefix
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',')
        .collect();
    if clean.is_empty() {
        return Ok(Vec::new());
    }
    if clean.len() % 2 != 0 {
        return Err(format!(
            "HEX 长度必须为偶数 (每字节 2 个十六进制字符), 当前长度 {}",
            clean.len()
        ));
    }
    let mut bytes = Vec::with_capacity(clean.len() / 2);
    let chars: Vec<char> = clean.chars().collect();
    for i in (0..chars.len()).step_by(2) {
        let s: String = chars[i..i + 2].iter().collect();
        let b = u8::from_str_radix(&s, 16)
            .map_err(|_| format!("无效的 HEX 字节: {}", s))?;
        bytes.push(b);
    }
    Ok(bytes)
}

/// 解析 ASCII 文本 + 转义字符 (\n \r \t \xHH \0 \\)
pub fn parse_ascii(input: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            match next {
                'n' => {
                    bytes.push(0x0a);
                    i += 2;
                }
                'r' => {
                    bytes.push(0x0d);
                    i += 2;
                }
                't' => {
                    bytes.push(0x09);
                    i += 2;
                }
                '\\' => {
                    bytes.push(0x5c);
                    i += 2;
                }
                '0' => {
                    bytes.push(0x00);
                    i += 2;
                }
                'x' if i + 3 < chars.len() => {
                    let hex: String = chars[i + 2..i + 4].iter().collect();
                    if let Ok(b) = u8::from_str_radix(&hex, 16) {
                        bytes.push(b);
                        i += 4;
                    } else {
                        bytes.push(ch as u8);
                        i += 1;
                    }
                }
                _ => {
                    bytes.push(ch as u8);
                    i += 1;
                }
            }
        } else {
            // 非 ASCII 字符 (>127) 用 UTF-8 编码
            let s: String = ch.to_string();
            for b in s.bytes() {
                bytes.push(b);
            }
            i += 1;
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_format_hex() {
        assert_eq!(detect_format("AA 01 02 BB"), InputFormat::Hex);
        assert_eq!(detect_format("AA0102BB"), InputFormat::Hex);
        assert_eq!(detect_format("0xAA 0x01"), InputFormat::Hex);
        assert_eq!(detect_format("AA,01,02"), InputFormat::Hex);
    }

    #[test]
    fn test_detect_format_ascii() {
        // 包含非 hex 字符 → Ascii
        assert_eq!(detect_format("1.0,2.0,3.0\\n"), InputFormat::Ascii);
        assert_eq!(detect_format("Hello"), InputFormat::Ascii);
        assert_eq!(detect_format("t1234\\r"), InputFormat::Ascii);
        // 奇数长度 → Ascii
        assert_eq!(detect_format("ABC"), InputFormat::Ascii);
        // 空 → Ascii (默认)
        assert_eq!(detect_format(""), InputFormat::Ascii);
    }

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

    #[test]
    fn test_parse_hex_spaces() {
        assert_eq!(parse_hex("AA 01 02 BB").unwrap(), vec![0xAA, 0x01, 0x02, 0xBB]);
    }

    #[test]
    fn test_parse_hex_compact() {
        assert_eq!(parse_hex("AA0102BB").unwrap(), vec![0xAA, 0x01, 0x02, 0xBB]);
    }

    #[test]
    fn test_parse_hex_commas() {
        assert_eq!(parse_hex("AA,01,02").unwrap(), vec![0xAA, 0x01, 0x02]);
    }

    #[test]
    fn test_parse_hex_with_0x_prefix() {
        assert_eq!(parse_hex("0xAA 0x01").unwrap(), vec![0xAA, 0x01]);
    }

    #[test]
    fn test_parse_hex_odd_length_error() {
        assert!(parse_hex("ABC").is_err());
    }

    #[test]
    fn test_parse_hex_invalid_char_error() {
        assert!(parse_hex("ZZ").is_err());
    }

    #[test]
    fn test_parse_ascii_plain() {
        assert_eq!(parse_ascii("Hello"), vec![b'H', b'e', b'l', b'l', b'o']);
    }

    #[test]
    fn test_parse_ascii_escapes() {
        assert_eq!(parse_ascii("a\\nb\\nc"), vec![b'a', 0x0a, b'b', 0x0a, b'c']);
        assert_eq!(parse_ascii("\\t\\r\\0\\\\"), vec![0x09, 0x0d, 0x00, 0x5c]);
    }

    #[test]
    fn test_parse_ascii_hex_escape() {
        assert_eq!(parse_ascii("\\xAA\\x01"), vec![0xAA, 0x01]);
    }

    #[test]
    fn test_parse_ascii_utf8() {
        // 中文字符 "中" UTF-8 = E4 B8 AD
        assert_eq!(parse_ascii("中"), vec![0xE4, 0xB8, 0xAD]);
    }

    // ===== parse_input 跨协议测试 =====

    use crate::{CandleEngine, FireWaterEngine, JustFloatEngine, RawDataEngine, SlcanEngine};
    use vofa_next_core::CanDirection;

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
