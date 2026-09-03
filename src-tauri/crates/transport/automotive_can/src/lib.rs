//! `automotive_can` — CAN 后端桥接 (Slcan / CandleLight → 统一 `CanBackend`)
//!
//! 在 `transport_core` 的 `TransportManager` 字节流之上,
//! 装配 `SlcanEngine` / `CandleEngine`,把解码出的 `CanFrame` 广播给上层
//! 诊断引擎,发送方向则把 `CanFrame` 编码回字节流送入 transport。

mod bridge;

pub use bridge::{BackendKind, BridgeCanBackend};
