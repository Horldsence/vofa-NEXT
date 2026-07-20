use vofa_next_core::Result;
use vofa_next_protocol::InputFormat;

/// 帧解码器手动测试结果 (与前端 FrameDecoderManualResult 对应)
#[derive(Debug, Clone, serde::Serialize)]
pub struct FrameDecoderManualResult {
    /// 端口名 → 值 (来自 field/bitfield/length/id 块)
    pub outputs: std::collections::HashMap<String, f32>,
    /// 校验是否通过
    pub valid: bool,
    /// 本帧消耗的字节数 (header + 所有 blocks)
    pub consumed_bytes: usize,
    /// 错误信息 (Hex 解析失败 / 帧头未找到 / 帧不完整等)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 解析用户输入字符串为帧 (用于 FrameDecoder 手动测试模式)
///
/// 使用 blocks 配置创建临时 FrameParser, 调用 parse_once_with_consumed
/// 返回 outputs + valid + consumed_bytes + 可选 error
pub async fn parse_frame_decoder_input(
    blocks: Vec<vofa_next_nodes::DecoderBlockDef>,
    input: String,
    format: InputFormat,
    enable_valid: bool,
    enable_frame_count: bool,
    enable_last_timestamp: bool,
    enable_fps: bool,
) -> Result<FrameDecoderManualResult> {
    use vofa_next_nodes::FrameParser;
    use vofa_next_protocol::engine::{detect_format, parse_ascii, parse_hex};

    // 1. 解析输入字符串为字节
    let actual_format = match format {
        InputFormat::Auto => detect_format(&input),
        f => f,
    };
    let bytes = match actual_format {
        InputFormat::Hex => match parse_hex(&input) {
            Ok(b) => b,
            Err(e) => {
                return Ok(FrameDecoderManualResult {
                    outputs: std::collections::HashMap::new(),
                    valid: false,
                    consumed_bytes: 0,
                    error: Some(e),
                });
            }
        },
        InputFormat::Ascii => parse_ascii(&input),
        InputFormat::Auto => unreachable!(),
    };

    // 2. 创建临时 FrameParser (无状态, 仅用于一次性解析)
    let parser = FrameParser::new(
        blocks,
        enable_valid,
        enable_frame_count,
        enable_last_timestamp,
        enable_fps,
    );

    // 3. 解析一帧 — 使用当前系统时间作为时间戳 (微秒)
    let now_us = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0);

    match parser.parse_once_with_consumed(&bytes, now_us) {
        Some((frame, consumed)) => Ok(FrameDecoderManualResult {
            outputs: frame.outputs,
            valid: frame.valid,
            consumed_bytes: consumed,
            error: None,
        }),
        None => Ok(FrameDecoderManualResult {
            outputs: std::collections::HashMap::new(),
            valid: false,
            consumed_bytes: 0,
            error: Some("无法解析: 未找到帧头或帧不完整".to_string()),
        }),
    }
}
