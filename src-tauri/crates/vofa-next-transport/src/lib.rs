//! VOFA-NEXT 传输层 façade
//!
//! 聚合子 crate,保留旧的 `use vofa_next_transport::*` 调用路径:
//! - `transport_core` — TransportHandle + TransportManager + CanBackend + test_data
//! - `transport_serial` — 串口 + Windows COM Description
//! - `transport_net` — TCP/UDP
//! - `transport_can_bridge` — Slcan + CandleLight (含 `candle` 子模块转发, 保
//!   `vofa_next_transport::candle::list_devices()` 旧路径可用)

pub use transport_can_bridge::candle;
pub use transport_core::{CanBackend, TransportHandle, TransportManager};