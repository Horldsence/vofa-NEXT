//! # commands — Tauri 命令 thin facade (Stage H 后)
//!
//! 13 个子模块已迁入 7 个 `cmd_*` sub-crate (`cmd_buffer` / `cmd_can_load` /
//! `cmd_can_transport` / `cmd_debug` / `cmd_graph` / `cmd_pipeline` / `cmd_rawdata`)。
//! 本文件保留 `commands::function` 命名空间, 给 `lib.rs::run` 中
//! `tauri::generate_handler![commands::xxx, ...]` 旧的调用路径,
//! 让 src-tauri 不直接知道命令分在哪个 cmd_* crate。
//!
//! 新代码应直接 `use cmd_<domain>::function` 拿具体命令函数。

pub use cmd_buffer::*;
pub use cmd_can_load::*;
pub use cmd_can_transport::*;
pub use cmd_debug::*;
pub use cmd_graph::*;
pub use cmd_pipeline::*;
pub use cmd_rawdata::*;
