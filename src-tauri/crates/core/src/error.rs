//! # 统一错误类型
//!
//! 跨 crate 的核心错误枚举。所有 `Result<T>` 默认使用 [`Result`] = `Result<T, Error>`。
//!
//! ## 序列化约定
//!
//! `Error` 实现 `serde::Serialize` 为 `{ "kind": "<VariantName>", "message": "..." }`,
//! 与前端 `NodeErrorKind` 枚举一一对应,便于 IPC 透传。
//!
//! ## 错误分类
//!
//! - `Transport` / `Protocol` / `Config`: 业务级字符串错误
//! - `PortNotFound` / `PortAlreadyOpen` / `PortNotOpen`: 端口状态错误
//! - `Io`: 自动转换 `std::io::Error`
//! - `Serde`: 自动转换 `serde_json::Error`

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("传输错误: {0}")]
    Transport(String),

    #[error("协议错误: {0}")]
    Protocol(String),

    #[error("端口未找到: {0}")]
    PortNotFound(String),

    #[error("端口已打开: {0}")]
    PortAlreadyOpen(String),

    #[error("端口未打开: {0}")]
    PortNotOpen(String),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("配置错误: {0}")]
    Config(String),

    #[error("序列化错误: {0}")]
    Serde(#[from] serde_json::Error),
}

/// 默认 `Result<T>` 别名 — 业务代码 `Result<T>` 自动指向此处。
pub type Result<T> = std::result::Result<T, Error>;

impl serde::Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("kind", self.kind())?;
        map.serialize_entry("message", &self.to_string())?;
        map.end()
    }
}

impl Error {
    /// 枚举变体名 — 跨 IPC 传递, 前端 `NodeErrorKind` 与之对应
    pub const fn kind(&self) -> &'static str {
        match self {
            Error::Transport(_) => "Transport",
            Error::Protocol(_) => "Protocol",
            Error::PortNotFound(_) => "PortNotFound",
            Error::PortAlreadyOpen(_) => "PortAlreadyOpen",
            Error::PortNotOpen(_) => "PortNotOpen",
            Error::Io(_) => "Io",
            Error::Config(_) => "Config",
            Error::Serde(_) => "Serde",
        }
    }
}
