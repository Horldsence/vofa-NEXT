use thiserror::Error;

/// 诊断引擎错误
#[derive(Debug, Error)]
pub enum AutomotiveError {
    #[error("ISO-TP 错误: {0}")]
    IsoTp(String),

    #[error("UDS 错误: {0}")]
    Uds(String),

    #[error("OBD-II 错误: {0}")]
    Obd(String),

    #[error("J1939 错误: {0}")]
    J1939(String),

    #[error("CAN 后端错误: {0}")]
    Backend(String),

    #[error("超时: {0}")]
    Timeout(String),

    #[error("参数无效: {0}")]
    Invalid(String),
}

pub type AutomotiveResult<T> = Result<T, AutomotiveError>;
