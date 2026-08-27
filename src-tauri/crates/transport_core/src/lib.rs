//! 传输编排层 — TransportHandle + TransportManager + CanBackend trait + 测试数据生成
//!
//! 负责按节点 ID 注册多路传输连接,统一收发/统计/状态接口。
//! 各类后端实现下沉到 `transport_serial`/`transport_net`/`transport_can_bridge` 子 crate。

pub mod can_backend;
pub mod handle;
pub mod manager;
pub mod test_data;

pub use can_backend::CanBackend;
pub use handle::TransportHandle;
pub use manager::TransportManager;
