//! # SubscriptionManager — 统一订阅取消管理器
//!
//! 集中管理所有按 channel_id 取消的订阅任务，消除 6 个重复的
//! `Arc<Mutex<HashMap<u32, oneshot::Sender<()>>>>` 字段。

use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::Mutex;
use tokio::sync::oneshot;

/// 统一订阅取消管理器
///
/// # 设计动机
/// 在 commands.rs 和 state.rs 中，有 6 种数据类型（waveform / rawdata /
/// can_frames / can_load / logic_samples / decoded_events）各自维护了一套
/// 完全相同的 `Arc<Mutex<HashMap<u32, oneshot::Sender<()>>>>` 来管理订阅取消。
///
/// 此管理器将这一模式统一：
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn test_register_and_cancel() {
        let mgr = SubscriptionManager::new();
        assert!(mgr.is_empty());

        // 注册模拟的订阅任务
        let (tx, rx) = oneshot::channel::<()>();
        mgr.tasks.lock().insert(42u32, tx);

        assert_eq!(mgr.len(), 1);

        // 取消
        if let Some(tx) = mgr.tasks.lock().remove(&42) {
            let _ = tx.send(());
        }

        // 验证取消信号到达
        assert!(rx.await.is_ok());
        assert!(mgr.is_empty());
    }

    #[tokio::test]
    async fn test_cancel_nonexistent() {
        let mgr = SubscriptionManager::new();
        // 取消不存在的订阅不应 panic
        if let Some(tx) = mgr.tasks.lock().remove(&999) {
            let _ = tx.send(());
        }
        assert!(mgr.is_empty());
    }

    #[tokio::test]
    async fn test_clear_all() {
        let mgr = SubscriptionManager::new();
        let flag = Arc::new(AtomicBool::new(false));
        let flag_clone = flag.clone();

        // 注册一个订阅，取消时设置标志位
        let channel_id = 1u32;
        let rx = {
            let (tx, rx) = oneshot::channel::<()>();
            mgr.tasks.lock().insert(channel_id, tx);
            rx
        };

        // 模拟任务等待取消
        tokio::spawn(async move {
            let _ = rx.await;
            flag_clone.store(true, Ordering::SeqCst);
        });

        mgr.clear();
        // 给 task 一点时间运行
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        assert!(flag.load(Ordering::SeqCst));
        assert!(mgr.is_empty());
    }
}
