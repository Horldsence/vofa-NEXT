//! 网络传输层 — TCP (client/server) + UDP socket
//!
//! 由 `transport_core::TransportManager::open` 在 `TransportConfig::TcpClient`/
//! `TcpServer`/`Udp` 分支调用。返回 `(write_tx, data_tx, cancel)`,与其它
//! 传输后端统一的运行时接口。

pub mod tcp;
pub mod udp;