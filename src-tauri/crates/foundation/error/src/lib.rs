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

mod ai;
mod compile;
mod config;
mod port;
mod protocol;
mod transport;

pub use ai::{AiError, McpError};
pub use compile::{CompileError, CompileReport, PortDomain};
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

    /// 图编译错误,通过 `Boxed` 持有,避免 `error` 引入 `engine` 依赖。
    /// 源 crate 实现 `From<CompileError> for AppError`。
    #[error("图编译错误: {0}")]
    Graph(Boxed),

    /// AI 对话错误 (LLM provider 调用 / 消息转换)。
    #[error(transparent)]
    Ai(#[from] AiError),

    /// MCP 桥接错误 (外部 server 连接 / 本地 server 生命周期)。
    #[error(transparent)]
    Mcp(#[from] McpError),

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
            // AI / MCP 错误透传内层细粒度种类 (如 AiMissingApiKey / McpPersist),
            // 供前端本地化与结构化处理
            Self::Ai(inner) => inner.kind(),
            Self::Mcp(inner) => inner.kind(),
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

impl AppError {
    /// 变体结构化字段透传 — 前端可读数据 (port / host / adapter / model 等)。
    ///
    /// IPC 序列化 ([`DataView`]) 与 AI 错误事件的 `data` 字段共用,
    /// 仅供展示 / 本地化, 不承载敏感信息。
    pub fn data_fields(&self) -> std::collections::BTreeMap<&'static str, String> {
        match self {
            Self::PortNotFound(PortNotFoundError { port })
            | Self::PortAlreadyOpen(PortAlreadyOpenError { port })
            | Self::PortNotOpen(PortNotOpenError { port }) => {
                std::collections::BTreeMap::from([("port", port.clone())])
            }
            Self::Transport(
                TransportError::SerialOpen { port, .. }
                | TransportError::SlcanOpen { port, .. }
                | TransportError::CandleOpen { port, .. },
            ) => std::collections::BTreeMap::from([("port", port.clone())]),
            Self::Transport(TransportError::TcpConnect { host, port, .. }) => {
                std::collections::BTreeMap::from([
                    ("host", host.clone()),
                    ("port", port.to_string()),
                ])
            }
            Self::Transport(
                TransportError::TcpListen { addr, .. }
                | TransportError::UdpBind { addr, .. }
                | TransportError::UdpConnect { addr, .. },
            ) => std::collections::BTreeMap::from([("addr", addr.clone())]),
            Self::Transport(TransportError::CanEncode { id, details }) => {
                std::collections::BTreeMap::from([
                    ("id", format!("{id:X}")),
                    ("details", details.clone()),
                ])
            }
            Self::Config(
                ConfigError::NodeNotFound { node_id }
                | ConfigError::ProtocolNodeNotFound { node_id }
                | ConfigError::GraphPortUnresolved { node_id, .. },
            ) => std::collections::BTreeMap::from([("node_id", node_id.clone())]),
            Self::Config(ConfigError::GraphVersionConflict { current }) => {
                std::collections::BTreeMap::from([("current", current.to_string())])
            }
            Self::Config(
                ConfigError::StreamGroupNotFound { key }
                | ConfigError::StreamGroupTypeMismatch { key },
            ) => std::collections::BTreeMap::from([("key", key.clone())]),
            Self::Config(ConfigError::StreamGroupFull { key, max }) => {
                std::collections::BTreeMap::from([("key", key.clone()), ("max", max.to_string())])
            }
            Self::Config(ConfigError::UrlParse { url, .. }) => {
                std::collections::BTreeMap::from([("url", url.clone())])
            }
            Self::Ai(
                AiError::MissingApiKey { adapter }
                | AiError::UnknownAdapter { adapter }
                | AiError::MissingModel { adapter },
            ) => std::collections::BTreeMap::from([("adapter", adapter.clone())]),
            Self::Ai(AiError::ProviderRequest { adapter, model, .. }) => {
                std::collections::BTreeMap::from([
                    ("adapter", adapter.clone()),
                    ("model", model.clone()),
                ])
            }
            Self::Ai(AiError::MaxToolRounds { rounds }) => {
                std::collections::BTreeMap::from([("rounds", rounds.to_string())])
            }
            Self::Ai(AiError::UnknownSession { id }) => {
                std::collections::BTreeMap::from([("id", id.clone())])
            }
            _ => std::collections::BTreeMap::new(),
        }
    }
}

/// 变体字段的透传视图 — 前端可读结构化数据(port / host / edge_id 等)。
struct DataView<'a>(&'a AppError);

impl serde::Serialize for DataView<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        self.0.data_fields().serialize(s)
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
    fn ai_app_error_kind_delegates_to_inner() {
        let e: AppError = AiError::MissingApiKey {
            adapter: "openai".into(),
        }
        .into();
        assert_eq!(e.kind(), "AiMissingApiKey");
        let e: AppError = McpError::Persist {
            source: std::io::Error::other("x"),
        }
        .into();
        assert_eq!(e.kind(), "McpPersist");
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
    fn ai_error_data_fields_expose_structured_params() {
        let e: AppError = AiError::MissingApiKey {
            adapter: "orcarouter".into(),
        }
        .into();
        assert_eq!(
            e.data_fields().get("adapter").map(String::as_str),
            Some("orcarouter")
        );

        let e: AppError = AiError::ProviderRequest {
            adapter: "orcarouter".into(),
            model: "openai/gpt-4o-mini".into(),
            source: Box::new(std::io::Error::other("401")),
        }
        .into();
        let fields = e.data_fields();
        assert_eq!(
            fields.get("adapter").map(String::as_str),
            Some("orcarouter")
        );
        assert_eq!(
            fields.get("model").map(String::as_str),
            Some("openai/gpt-4o-mini")
        );

        let e: AppError = AiError::MaxToolRounds { rounds: 8 }.into();
        assert_eq!(e.data_fields().get("rounds").map(String::as_str), Some("8"));
        // 顶层 kind 委托内层细粒度种类 (IPC 错误对象与 AI 错误事件共用)
        assert_eq!(e.kind(), "AiMaxToolRounds");

        let e: AppError = AiError::Keyring {
            details: "locked".into(),
        }
        .into();
        assert_eq!(e.kind(), "AiKeyring");

        let e: AppError = AiError::KeyringAccessDenied {
            details: "cancelled".into(),
        }
        .into();
        assert_eq!(e.kind(), "AiKeyringAccessDenied");
        let value = serde_json::to_value(&e).expect("serialize keyring access denied");
        assert_eq!(value["kind"], "AiKeyringAccessDenied");
        assert!(value["data"]
            .as_object()
            .is_some_and(serde_json::Map::is_empty));
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
