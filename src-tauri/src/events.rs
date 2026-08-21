//! 前端事件契约 — re-export 自 `notify_events` crate
//!
//! Stage H 拆分后, 原 `events.rs` 已迁至 `crates/notify_events/src/lib.rs`。
//! 本文件作为薄 facade 维持 `crate::events::emit_transport_*` 等旧调用路径可用,
//! 新代码可直接 `use notify_events::*`。

pub use notify_events::{emit_transport_state, TransportRxEvent, TransportStateEvent};
