//! # subscription — 统一订阅管理
//!
//! 所有显示数据类型共享一个 [`SubscriptionManager`]，消除按类型维护取消表的重复模式。
//!
//! 每个订阅在 `register()` 时获得一个 `oneshot::Receiver<()>`，
//! 调用 `cancel(channel_id)` 触发取消信号，后台 task 在 `select!` 中收到后优雅退出。
//!
mod manager;

pub use manager::SubscriptionManager;

use tokio::sync::oneshot;

/// 注册一个订阅，返回取消接收端
///
/// # 参数
/// * `manager` — 统一订阅管理器
/// * `channel_id` — Tauri Channel 的 id
///
/// # 返回
/// `oneshot::Receiver<()>`，task 中与 ticker 做 select! 等待取消
pub fn register_cancel(manager: &SubscriptionManager, channel_id: u32) -> oneshot::Receiver<()> {
    let (tx, rx) = oneshot::channel();
    manager.tasks.lock().insert(channel_id, tx);
    rx
}

/// 发起订阅取消
pub fn cancel_subscription(manager: &SubscriptionManager, channel_id: u32) {
    let tx = manager.tasks.lock().remove(&channel_id);
    if let Some(tx) = tx {
        let _ = tx.send(());
    }
}

/// 清理订阅记录（不发送取消信号，用于通道已关闭后的清理）
pub fn remove_subscription(manager: &SubscriptionManager, channel_id: u32) {
    manager.tasks.lock().remove(&channel_id);
}
