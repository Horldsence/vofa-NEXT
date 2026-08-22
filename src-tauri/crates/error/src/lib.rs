//! 统一错误抽象 — `Error` trait + `AppError` 顶级枚举 + 各领域强类型错误。
//!
//! ## 设计原则
//!
//! 1. **trait 抽象**:[`Error`] 是稳定契约,提供 `kind()` / `status()` / `source()`。
//!    所有领域错误类型实现该 trait,跨 IPC 序列化统一。
//! 2. **无 catch-all 字符串**:`Error::Foo(String)` 反模式被禁止。字符串仅作为
//!    结构化字段(`port` / `host` / `details` 等)承载真实数据。
//! 3. **零循环依赖**:本 crate 仅依赖 `serde` / `serde_json` / `thiserror`。
//!    跨领域引用(`AutomotiveError` / `CompileError`)通过 [`Boxed`] 持有,避免
//!    `error → domain → vofa_core → error` 环。
//! 4. **`#[from]` 自动转换**:`AppError` 顶层枚举对每个内部错误类型提供 `From`,
//!    调用方 `?` 直传,无需 `map_err` 模板。

use std::error::Error as StdError;

use serde::ser::{SerializeMap, Serializer};
use thiserror::Error as ThisError;

mod config;
mod port;
mod protocol;
mod transport;

pub use config::ConfigError;
pub use port::{PortAlreadyOpenError, PortNotFoundError, PortNotOpenError};
pub use protocol::ProtocolError;
pub use transport::TransportError;

/// 跨 IPC 错误抽象。所有领域错误类型实现该 trait。
///
/// 与 [`std::error::Error`] 的关系:`Error` 是 `StdError` 的扩展,`kind()`
/// 提供 IPC 序列化所需的稳定字符串标识,与前端 `NodeErrorKind` 枚举对应。
///
/// 无 blanket impl(避免 specialization 不稳定);`std::io::Error` /
/// `serde_json::Error` 等 foreign 类型在 [`impls`] 模块手写 impl。
pub trait Error: StdError + Send + Sync + 'static {
    /// 跨 IPC 错误种类。
    fn kind(&self) -> &'static str;

    /// HTTP 风格状态码 (预留,默认 `None`)。
    fn status(&self) -> Option<u16> {
        None
    }

    /// 重导出 `StdError::source` 便于 trait object 调用。
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        StdError::source(self)
    }
}

/// 兜底 boxed 错误,用于跨 crate 引用(避免 `error` 引入 domain crate 依赖)。
///
/// 持有 `dyn StdError + Send + Sync` 而非 `dyn Error`:
/// - `Box<T>` 在 `T: StdError + ?Sized` 时自动 impl `StdError`,thiserror
///   的 `#[source]` 宏可直接展开
/// - `kind()` 由 `AppError` 变体本身决定,不依赖 boxed 内层类型
pub type Boxed = Box<dyn StdError + Send + Sync>;

/// 第三方插件错误 (tauri-plugin-* 等不可控边界)。
#[derive(Debug, ThisError)]
#[error("插件错误 [{plugin}]: {source}")]
pub struct PluginError {
    pub plugin: &'static str,
    #[source]
    pub source: Box<dyn StdError + Send + Sync>,
}

impl Error for PluginError {
    fn kind(&self) -> &'static str {
        "Plugin"
    }
}

/// 跨 crate 统一错误类型 — `Result<T>` 默认指向此处。
#[derive(Debug, ThisError)]
pub enum AppError {
    #[error(transparent)]
    Transport(#[from] TransportError),

    #[error(transparent)]
    Protocol(#[from] ProtocolError),

    #[error(transparent)]
    PortNotFound(#[from] PortNotFoundError),

    #[error(transparent)]
    PortAlreadyOpen(#[from] PortAlreadyOpenError),

    #[error(transparent)]
    PortNotOpen(#[from] PortNotOpenError),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error(transparent)]
    Serde(#[from] serde_json::Error),

    /// 汽车诊断 / ISO-TP / UDS 错误,通过 `Boxed` 持有,避免 `error` 引入
    /// `automotive_isotp` 依赖。源 crate 实现 `From<AutomotiveError> for AppError`。
    #[error("汽车诊断错误: {0}")]
    Automotive(Boxed),

    /// 图编译错误,通过 `Boxed` 持有,避免 `error` 引入 `node_engine` 依赖。
    /// 源 crate 实现 `From<CompileError> for AppError`。
    #[error("图编译错误: {0}")]
    Graph(Boxed),

    /// 第三方插件错误 (不可控边界)。
    #[error(transparent)]
    Plugin(#[from] PluginError),

    /// 兜底 — 来自其它领域的未分类 `Boxed` 错误。
    #[error("其他错误: {0}")]
    Other(Boxed),
}

/// 默认 `Result<T>` 别名 — 业务代码 `Result<T>` 自动指向此处。
pub type Result<T> = std::result::Result<T, AppError>;

/// `std::io::Error` 的 `Error` impl — foreign 类型手写覆盖 (避免 specialization)。
impl Error for std::io::Error {
    fn kind(&self) -> &'static str {
        "Io"
    }
}

/// `serde_json::Error` 的 `Error` impl。
impl Error for serde_json::Error {
    fn kind(&self) -> &'static str {
        "Serde"
    }
}

impl Error for AppError {
    fn kind(&self) -> &'static str {
        match self {
            Self::Transport(_) => "Transport",
            Self::Protocol(_) => "Protocol",
            Self::PortNotFound(_) => "PortNotFound",
            Self::PortAlreadyOpen(_) => "PortAlreadyOpen",
            Self::PortNotOpen(_) => "PortNotOpen",
            Self::Io(_) => "Io",
            Self::Config(_) => "Config",
            Self::Serde(_) => "Serde",
            Self::Automotive(_) => "Automotive",
            Self::Graph(_) => "Graph",
            Self::Plugin(_) => "Plugin",
            Self::Other(_) => "Other",
        }
    }

    fn status(&self) -> Option<u16> {
        match self {
            Self::PortNotFound(_) | Self::PortNotOpen(_) => Some(404),
            Self::PortAlreadyOpen(_) => Some(409),
            Self::Io(_) => Some(502),
            _ => None,
        }
    }
}

impl serde::Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(4))?;
        map.serialize_entry("kind", self.kind())?;
        map.serialize_entry("message", &self.to_string())?;
        if let Some(src) = StdError::source(self) {
            map.serialize_entry("source", &SourceView(src))?;
        }
        map.serialize_entry("data", &DataView(self))?;
        map.end()
    }
}

/// 错误链上一层的简化视图 — 仅 `message`,避免循环引用与 trait object 类型擦除。
struct SourceView<'a>(&'a dyn StdError);

impl serde::Serialize for SourceView<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(1))?;
        map.serialize_entry("message", &self.0.to_string())?;
        map.end()
    }
}

/// 变体字段的透传视图 — 前端可读结构化数据(port / host / edge_id 等)。
struct DataView<'a>(&'a AppError);

impl serde::Serialize for DataView<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let map: std::collections::BTreeMap<&'static str, String> = match self.0 {
            AppError::PortNotFound(PortNotFoundError { port })
            | AppError::PortAlreadyOpen(PortAlreadyOpenError { port })
            | AppError::PortNotOpen(PortNotOpenError { port }) => {
                std::collections::BTreeMap::from([("port", port.clone())])
            }
            AppError::Transport(
                TransportError::SerialOpen { port, .. }
                | TransportError::SlcanOpen { port, .. }
                | TransportError::CandleOpen { port, .. },
            ) => std::collections::BTreeMap::from([("port", port.clone())]),
            AppError::Transport(TransportError::TcpConnect { host, port, .. }) => {
                std::collections::BTreeMap::from([
                    ("host", host.clone()),
                    ("port", port.to_string()),
                ])
            }
            AppError::Transport(
                TransportError::TcpListen { addr, .. }
                | TransportError::UdpBind { addr, .. }
                | TransportError::UdpConnect { addr, .. },
            ) => std::collections::BTreeMap::from([("addr", addr.clone())]),
            AppError::Transport(TransportError::CanEncode { id, details }) => {
                std::collections::BTreeMap::from([
                    ("id", format!("{id:X}")),
                    ("details", details.clone()),
                ])
            }
            AppError::Config(
                ConfigError::NodeNotFound { node_id }
                | ConfigError::ProtocolNodeNotFound { node_id },
            ) => std::collections::BTreeMap::from([("node_id", node_id.clone())]),
            AppError::Config(
                ConfigError::StreamGroupNotFound { key }
                | ConfigError::StreamGroupTypeMismatch { key },
            ) => std::collections::BTreeMap::from([("key", key.clone())]),
            AppError::Config(ConfigError::StreamGroupFull { key, max }) => {
                std::collections::BTreeMap::from([("key", key.clone()), ("max", max.to_string())])
            }
            AppError::Config(ConfigError::UrlParse { url, .. }) => {
                std::collections::BTreeMap::from([("url", url.clone())])
            }
            _ => std::collections::BTreeMap::new(),
        };
        map.serialize(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_error_display_uses_transport_message() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let e: AppError = TransportError::SerialOpen {
            port: "COM3".into(),
            source: io_err,
        }
        .into();
        let msg = e.to_string();
        assert!(msg.contains("COM3"));
        assert!(msg.contains("denied"));
    }

    #[test]
    fn app_error_kind_for_each_variant() {
        assert_eq!(
            AppError::Transport(TransportError::SerialClone(std::io::Error::other("x"))).kind(),
            "Transport"
        );
        assert_eq!(
            AppError::PortNotFound(PortNotFoundError { port: "x".into() }).kind(),
            "PortNotFound"
        );
        assert_eq!(
            AppError::PortAlreadyOpen(PortAlreadyOpenError { port: "x".into() }).kind(),
            "PortAlreadyOpen"
        );
        assert_eq!(
            AppError::PortNotOpen(PortNotOpenError { port: "x".into() }).kind(),
            "PortNotOpen"
        );
        let io_err = std::io::Error::other("x");
        assert_eq!(AppError::Io(io_err).kind(), "Io");
        assert_eq!(
            AppError::Serde(serde_json::from_str::<i32>("\"x\"").unwrap_err()).kind(),
            "Serde"
        );
    }

    #[test]
    fn app_error_status_codes() {
        let port_err = PortNotFoundError { port: "x".into() };
        assert_eq!(AppError::PortNotFound(port_err).status(), Some(404));
        let taken = PortAlreadyOpenError { port: "x".into() };
        assert_eq!(AppError::PortAlreadyOpen(taken).status(), Some(409));
        assert_eq!(AppError::Io(std::io::Error::other("x")).status(), Some(502));
        assert_eq!(
            AppError::Serde(serde_json::from_str::<i32>("\"x\"").unwrap_err()).status(),
            None
        );
    }

    #[test]
    fn app_error_serializes_kind_message_source_data() {
        let io_err = std::io::Error::other("boom");
        let e: AppError = TransportError::SerialOpen {
            port: "COM3".into(),
            source: io_err,
        }
        .into();
        let v = serde_json::to_value(&e).expect("serialize");
        assert_eq!(v["kind"], "Transport");
        assert!(v["message"].as_str().unwrap().contains("COM3"));
        assert!(v["source"]["message"].as_str().unwrap().contains("boom"));
        assert_eq!(v["data"]["port"], "COM3");
    }

    #[test]
    fn boxed_error_roundtrip() {
        let e: AppError = TransportError::SerialClone(std::io::Error::other("x")).into();
        let boxed: Boxed = Box::new(TransportError::SerialClone(std::io::Error::other("y")));
        let other: AppError = AppError::Other(boxed);
        assert_eq!(other.kind(), "Other");
        assert!(other.to_string().contains('y'));
        assert_eq!(e.kind(), "Transport");
    }

    #[test]
    fn plugin_error_kind() {
        let p = PluginError {
            plugin: "updater",
            source: Box::new(std::io::Error::other("net")),
        };
        let e: AppError = p.into();
        assert_eq!(e.kind(), "Plugin");
        assert!(e.to_string().contains("updater"));
    }
}
