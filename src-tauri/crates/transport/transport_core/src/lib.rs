//! 传输编排层 — TransportHandle + TransportManager + CanBackend trait + 测试数据生成
//!
//! 负责按节点 ID 注册多路传输连接,统一收发/统计/状态接口。
//! 各类后端实现下沉到 `serial`/`net`/`can_bridge` 子 crate。
//!
//! 对外暴露 `TransportManager` (编排) 与 `LiveNodeHandle` (数据平面轻量句柄);
//! `TransportHandle` 为 crate 内部细节, 不对外导出。

pub mod can_backend;
mod handle;
pub mod manager;
pub mod test_data;

pub use can_backend::CanBackend;
pub use handle::LiveNodeHandle;
pub use manager::TransportManager;
