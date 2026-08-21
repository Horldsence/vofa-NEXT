//! 按通道 (stable/beta) 检查应用更新。
//!
//! 两个通道各对应一个静态 manifest URL, 不经 GitHub API, 避免未认证
//! 请求 60 次/小时/出口 IP 的限流 (超额返回 403):
//! - stable: GitHub 最新正式 release 的 latest.json
//! - beta: 滚动 tag `beta` 上的 beta-latest.json,
//!   由 release CI 在每次发布(含正式版)时更新, 语义等同"所有 release 中取最新"
//!
//! 拿到 manifest 后交给 tauri-plugin-updater 完成版本比较与更新下载。

// 注意: 命令必须放在子模块中, 不能定义在 crate 根。
// `#[tauri::command]` 展开含 `#[macro_export]` (提升到 crate 根宏命名空间)
// 与 `pub use __cmd__*`; 若命令本身就在 crate 根, 同一宏命名空间内
// "定义 + 再导入" 冲突, 触发 E0255。
mod update;

pub use update::*;
