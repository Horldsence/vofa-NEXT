//! 端口状态错误 — `port` 字段是数据(端口名如 `/dev/ttyUSB0`),非 catch-all。

use thiserror::Error;

use crate::Error;

#[derive(Debug, Clone, Error)]
#[error("端口未找到: {port}")]
pub struct PortNotFoundError {
    pub port: String,
}

#[derive(Debug, Clone, Error)]
#[error("端口已打开: {port}")]
pub struct PortAlreadyOpenError {
    pub port: String,
}

#[derive(Debug, Clone, Error)]
#[error("端口未打开: {port}")]
pub struct PortNotOpenError {
    pub port: String,
}

impl Error for PortNotFoundError {
    fn kind(&self) -> &'static str {
        "PortNotFound"
    }
}

impl Error for PortAlreadyOpenError {
    fn kind(&self) -> &'static str {
        "PortAlreadyOpen"
    }
}

impl Error for PortNotOpenError {
    fn kind(&self) -> &'static str {
        "PortNotOpen"
    }
}
