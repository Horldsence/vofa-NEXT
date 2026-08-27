//! 协议解析/编码错误。

use thiserror::Error;

use crate::Error;

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("CRC 校验失败")]
    CrcMismatch,

    #[error("帧格式错误: {0}")]
    FrameFormat(String),

    #[error("协议解析失败: {0}")]
    Parse(String),

    #[error("协议编码失败: {0}")]
    Encode(String),
}

impl Error for ProtocolError {
    fn kind(&self) -> &'static str {
        "Protocol"
    }
}
