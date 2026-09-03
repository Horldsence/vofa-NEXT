//! `error` 抽象单元测试 — `core` crate 通过 re-export 暴露错误类型。
//!
//! 覆盖:
//! - 各变体 `kind()` 映射 (与前端 `NodeErrorKind` 对应)
//! - `Display` 文案 (前端 `message` 字段)
//! - 序列化约定 (`{kind, message, source, data}`)
//! - `Io` / `Serde` 自动 `From` 转换
//! - 业务错误构造的 `Display` 含结构化字段透传

use error::{
    ConfigError, Error as ErrorTrait, PortAlreadyOpenError, PortNotFoundError, PortNotOpenError,
    ProtocolError, TransportError,
};
use vofa_core::{Error, Result};

#[test]
fn display_text_for_transport_error() {
    let io = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
    let e = Error::Transport(TransportError::SerialOpen {
        port: "/dev/ttyUSB0".into(),
        source: io,
    });
    let s = e.to_string();
    assert!(s.contains("/dev/ttyUSB0"));
    assert!(s.contains("denied"));
}

#[test]
fn display_text_for_protocol_error() {
    let e = Error::Protocol(ProtocolError::CrcMismatch);
    assert_eq!(e.to_string(), "CRC 校验失败");
}

#[test]
fn display_text_for_port_errors() {
    let e = Error::PortNotFound(PortNotFoundError {
        port: "/dev/ttyUSB0".into(),
    });
    assert_eq!(e.to_string(), "端口未找到: /dev/ttyUSB0");
    let e = Error::PortAlreadyOpen(PortAlreadyOpenError {
        port: "COM3".into(),
    });
    assert_eq!(e.to_string(), "端口已打开: COM3");
    let e = Error::PortNotOpen(PortNotOpenError {
        port: "COM3".into(),
    });
    assert_eq!(e.to_string(), "端口未打开: COM3");
}

#[test]
fn display_text_for_config_error() {
    let e = Error::Config(ConfigError::NodeNotFound {
        node_id: "tab-x".into(),
    });
    assert_eq!(e.to_string(), "节点 tab-x 不存在");
}

#[test]
fn display_text_for_io_error_via_from() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
    let e: Error = io_err.into();
    assert!(e.to_string().starts_with("IO 错误:"));
}

#[test]
fn display_text_for_serde_error_via_from() {
    let bad = serde_json::from_str::<serde_json::Value>("{not json").unwrap_err();
    let e: Error = bad.into();
    assert_eq!(e.kind(), "Serde");
}

#[test]
fn kind_returns_variant_name_for_each_variant() {
    let io = std::io::Error::other("x");
    let e = Error::Transport(TransportError::SerialClone(io));
    assert_eq!(e.kind(), "Transport");
    let e = Error::Protocol(ProtocolError::CrcMismatch);
    assert_eq!(e.kind(), "Protocol");
    let e = Error::PortNotFound(PortNotFoundError {
        port: String::new(),
    });
    assert_eq!(e.kind(), "PortNotFound");
    let e = Error::PortAlreadyOpen(PortAlreadyOpenError {
        port: String::new(),
    });
    assert_eq!(e.kind(), "PortAlreadyOpen");
    let e = Error::PortNotOpen(PortNotOpenError {
        port: String::new(),
    });
    assert_eq!(e.kind(), "PortNotOpen");
    let e: Error = std::io::Error::other("x").into();
    assert_eq!(e.kind(), "Io");
    let e = Error::Config(ConfigError::AutoBindingMissingProtocolNode);
    assert_eq!(e.kind(), "Config");
    let bad = serde_json::from_str::<i32>("\"x\"").unwrap_err();
    let e: Error = bad.into();
    assert_eq!(e.kind(), "Serde");
}

#[test]
fn serializes_with_tagged_variant_name_and_message() {
    let e = Error::PortNotFound(PortNotFoundError {
        port: "/dev/ttyUSB0".into(),
    });
    let v = serde_json::to_value(&e).expect("serialize");
    assert_eq!(v["kind"], "PortNotFound");
    assert_eq!(v["message"], "端口未找到: /dev/ttyUSB0");
    assert_eq!(v["data"]["port"], "/dev/ttyUSB0");
}

#[test]
fn serializes_io_error_with_message() {
    let e: Error = std::io::Error::other("boom").into();
    let v = serde_json::to_value(&e).expect("serialize");
    assert_eq!(v["kind"], "Io");
    assert!(v["message"].as_str().unwrap().contains("boom"));
}

#[test]
fn result_alias_uses_core_error() {
    let ok_path: Result<u32> = Ok(7);
    let err_path: Result<u32> = Err(Error::Config(ConfigError::AutoBindingMissingProtocolNode));
    assert!(matches!(ok_path, Ok(7)));
    assert!(err_path.is_err());
}

#[test]
fn error_is_send_and_sync() {
    fn assert_send<T: Send + Sync>() {}
    assert_send::<Error>();
    assert_send::<Box<dyn ErrorTrait>>();
}

#[test]
fn error_debug_impl_present() {
    let e = Error::Transport(TransportError::SerialClone(std::io::Error::other("x")));
    let dbg = format!("{e:?}");
    assert!(dbg.contains("Transport"));
}
