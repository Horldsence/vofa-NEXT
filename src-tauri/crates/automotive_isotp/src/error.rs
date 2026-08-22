//! ISO-TP / 诊断引擎公共错误 — 强类型变体,无 `String` catch-all。

use thiserror::Error;

use error::AppError;

#[derive(Debug, Error)]
pub enum AutomotiveError {
    #[error("ISO-TP 帧 {tx_id:#x} N_As 超时")]
    IsoTpTimeout { tx_id: u32 },

    #[error("ISO-TP 流控帧 OVERFLOW")]
    IsoTpFlowControlOverflow,

    #[error("ISO-TP 会话已关闭")]
    IsoTpSessionClosed,

    #[error("ISO-TP 会话任务崩溃")]
    IsoTpTaskCrashed,

    #[error("ISO-TP 数据超长: {length} > {max}")]
    IsoTpDataTooLong { length: usize, max: usize },

    #[error("ISO-TP SN 不匹配: 期望 0x{expected:X} 收到 0x{got:X}")]
    IsoTpSequenceMismatch { expected: u8, got: u8 },

    #[error("CAN 后端发送失败: {0}")]
    BackendSend(#[source] std::io::Error),
}

impl error::Error for AutomotiveError {
    fn kind(&self) -> &'static str {
        "Automotive"
    }
}

impl From<AutomotiveError> for AppError {
    fn from(e: AutomotiveError) -> Self {
        Self::Automotive(Box::new(e))
    }
}

pub type AutomotiveResult<T> = Result<T, AutomotiveError>;