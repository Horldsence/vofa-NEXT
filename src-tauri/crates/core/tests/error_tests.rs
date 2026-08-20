//! `core::error` 单元测试
//!
//! 覆盖:
//! - 8 个变体的 `kind()` 映射
//! - Display 文案 (前端 message 字段)
//! - 序列化约定 (`{kind, message}`)
//! - Io/Serde 自动 `From` 转换
//! - 业务错误构造的 Display 含消息透传

use vofa_core::error::Error;
use vofa_core::Result;

#[test]
fn display_text_for_transport_error() {
    let e = Error::Transport("connection refused".into());
    assert_eq!(e.to_string(), "传输错误: connection refused");
}

#[test]
fn display_text_for_protocol_error() {
    let e = Error::Protocol("crc mismatch".into());
    assert_eq!(e.to_string(), "协议错误: crc mismatch");
}

#[test]
fn display_text_for_port_errors() {
    assert_eq!(
        Error::PortNotFound("/dev/ttyUSB0".into()).to_string(),
        "端口未找到: /dev/ttyUSB0"
    );
    assert_eq!(
        Error::PortAlreadyOpen("COM3".into()).to_string(),
        "端口已打开: COM3"
    );
    assert_eq!(
        Error::PortNotOpen("COM3".into()).to_string(),
        "端口未打开: COM3"
    );
}

#[test]
fn display_text_for_config_error() {
    let e = Error::Config("missing baud_rate".into());
    assert_eq!(e.to_string(), "配置错误: missing baud_rate");
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
    assert!(e.to_string().starts_with("序列化错误:"));
}

#[test]
fn kind_returns_variant_name_for_each_variant() {
    assert_eq!(Error::Transport("".into()).kind(), "Transport");
    assert_eq!(Error::Protocol("".into()).kind(), "Protocol");
    assert_eq!(Error::PortNotFound("".into()).kind(), "PortNotFound");
    assert_eq!(Error::PortAlreadyOpen("".into()).kind(), "PortAlreadyOpen");
    assert_eq!(Error::PortNotOpen("".into()).kind(), "PortNotOpen");
    assert_eq!(
        Error::Io(std::io::Error::new(std::io::ErrorKind::Other, "x")).kind(),
        "Io"
    );
    assert_eq!(Error::Config("".into()).kind(), "Config");
    assert_eq!(
        Error::Serde(serde_json::from_str::<i32>("\"x\"").unwrap_err()).kind(),
        "Serde"
    );
}

#[test]
fn serializes_with_tagged_variant_name_and_message() {
    let e = Error::PortNotFound("/dev/ttyUSB0".into());
    let v = serde_json::to_value(&e).expect("serialize");
    assert_eq!(v["kind"], "PortNotFound");
    assert_eq!(v["message"], "端口未找到: /dev/ttyUSB0");
}

#[test]
fn serializes_io_error_with_message() {
    let e: Error = std::io::Error::new(std::io::ErrorKind::Other, "boom").into();
    let v = serde_json::to_value(&e).expect("serialize");
    assert_eq!(v["kind"], "Io");
    assert!(v["message"].as_str().unwrap().contains("boom"));
}

#[test]
fn result_alias_uses_core_error() {
    fn ok_path() -> Result<u32> {
        Ok(7)
    }
    fn err_path() -> Result<u32> {
        Err(Error::Config("oops".into()))
    }
    assert_eq!(ok_path().unwrap(), 7);
    assert!(err_path().is_err());
}

#[test]
fn error_is_send_and_sync() {
    fn assert_send<T: Send + Sync>() {}
    assert_send::<Error>();
}

#[test]
fn error_debug_impl_present() {
    let e = Error::Transport("x".into());
    let dbg = format!("{e:?}");
    assert!(dbg.contains("Transport"));
}
