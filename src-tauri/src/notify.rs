//! 通知辅助模块 — re-export 自 `notify_events::notify` 子模块
//!
//! Stage H 拆分后, 原 `notify.rs` 已迁至 `crates/notify_events/src/notify.rs`。
//! 本文件作为薄 facade 维持 `crate::notify::*` 等旧调用路径可用,
//! 新代码可直接 `use notify_events::notify::*`。

pub use notify_events::notify::*;
