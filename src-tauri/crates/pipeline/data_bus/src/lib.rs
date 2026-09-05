//! Actor 化的数据 Topic 总线。
//!
//! 数据平面只负责发布有效样本；每个 Topic Actor 独占自己的环形历史、序号和
//! 订阅广播器。这样采集/图求值热路径不再与任意前端订阅共享数据锁。

mod actor;
mod adaptive;
mod bus;
mod types;

pub use adaptive::AdaptiveController;
pub use bus::DataBus;
pub use types::{RuntimeHealth, RuntimeLimits, Sample, SampleBatch, SampleStatus, TopicKey};
