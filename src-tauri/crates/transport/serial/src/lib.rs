//! 串口传输层 — 同步串口 + Windows COM Description 枚举
//!
//! 由 `transport_core::TransportManager::open` 在 `TransportConfig::Serial` 分支调用。
//! 返回 `(write_tx, data_tx, cancel)`,与其它传输后端统一的运行时接口。

pub mod serial;

#[cfg(windows)]
pub mod windows_ports;
