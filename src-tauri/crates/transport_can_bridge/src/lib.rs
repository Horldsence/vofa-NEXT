//! CAN 桥接传输层 — Slcan (ASCII over serial) + CandleLight (原生 USB bulk)
//!
//! 由 `transport_core::TransportManager::open` 在 `TransportConfig::Slcan`/
//! `CandleLight` 分支调用。返回 `(write_tx, data_tx, cancel)`,与其它
//! 传输后端统一的运行时接口。字节流透传到协议层由 `SlcanEngine`/
//! `CandleLightEngine` 解码成 CanFrame。

pub mod candle;
pub mod slcan;
