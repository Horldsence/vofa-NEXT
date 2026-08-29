//! # SubscriptionManager — 统一订阅取消管理器
//!
//! 集中管理所有按 channel_id 取消的显示订阅任务，消除重复的
//! `Arc<Mutex<HashMap<u32, oneshot::Sender<()>>>>` 字段。

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::oneshot;

/// 统一订阅取消管理器
///
/// 所有显示数据类型通过此管理器统一：
/// - `register(channel_id)` 创建取消 channel，返回 receiver 供 task 使用
/// - `cancel(channel_id)` 触发取消信号，task 收到后优雅退出
/// - 同一个 AppState 中只需要一个 `SubscriptionManager` 字段
///
/// # Clone 语义
/// `Clone` 共享内部 `Arc`，多个持有者操作同一个 HashMap。
#[derive(Clone)]
pub struct SubscriptionManager {
    pub(crate) tasks: Arc<Mutex<HashMap<u32, oneshot::Sender<()>>>>,
}

impl SubscriptionManager {
    /// 创建新的空管理器
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 返回已注册的任务数（用于调试和测试）
    pub fn len(&self) -> usize {
        self.tasks.lock().len()
    }

    /// 是否没有任何订阅
    pub fn is_empty(&self) -> bool {
        self.tasks.lock().is_empty()
    }

    /// 清除所有订阅（应用退出或重置时调用）
    pub fn clear(&self) {
        // 发送取消信号给所有活跃订阅
        let tasks = std::mem::take(&mut *self.tasks.lock());
        for (_, tx) in tasks {
            let _ = tx.send(());
        }
    }
}

impl Default for SubscriptionManager {
    fn default() -> Self {
        Self::new()
    }
}
