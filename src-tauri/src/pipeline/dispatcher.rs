//! # dispatcher — 自适应并发分发器
//!
//! 各数据主题 (波形/CAN/逻辑/原始字节/图输出…) 各占一个独立 tokio 任务,
//! 互不阻塞地通过 Tauri Channel 推送到前端 (并发 channel)。
//!
//! 所有分发循环共用 [`AdaptiveRate`] 做速率自适应:
//! - 每次实际发送 → 间隔减半 (不低于 min), 数据越快推得越勤
//! - 每次空转 (无数据变化) → 间隔 ×1.5 (不超过 max), 空闲自动降频退避
//!
//! 变化检测由调用方提供的 `make_batch` 闭包完成 (返回 `None` 表示无变化),
//! 通常基于各缓冲区的单调 version 计数器, 零额外拷贝。

use std::time::Duration;
use tauri::ipc::Channel;
use tokio::sync::oneshot;

/// 自适应推送速率控制器
///
/// `current` 在 `[min, max]` 内动态调整:
/// - `on_send()`: 有数据发出, 提速 (÷2, 不低于 min)
/// - `on_idle()`: 无数据变化, 降频 (×1.5, 不超过 max)
#[derive(Debug, Clone)]
pub struct AdaptiveRate {
    min: Duration,
    max: Duration,
    current: Duration,
}

impl AdaptiveRate {
    /// min: 最激进推送间隔; max: 空闲退避上限
    pub fn new(min: Duration, max: Duration) -> Self {
        debug_assert!(min <= max, "AdaptiveRate: min 必须 <= max");
        Self {
            min,
            max: max.max(min),
            current: min,
        }
    }

    /// 当前间隔
    pub fn current(&self) -> Duration {
        self.current
    }

    /// 记录一次实际发送 — 提速
    pub fn on_send(&mut self) {
        self.current = (self.current / 2).max(self.min);
    }

    /// 记录一次空转 (无变化) — 降频
    pub fn on_idle(&mut self) {
        self.current = (self.current + self.current / 2).min(self.max);
    }
}

/// 单订阅者自适应推送循环
///
/// 每个 wake 调用 `make_batch` (同步闭包, 内部做变化检测与快照):
/// - `Some(batch)` → 通过 Channel 发送, `rate.on_send()`; Channel 关闭则退出
/// - `None` → 不发送, `rate.on_idle()` 退避
///
/// 收到 `cancel_rx` 信号优雅退出。
///
/// `name` 仅用于日志。
pub async fn adaptive_channel_loop<T, F>(
    name: &str,
    channel_id: u32,
    on_event: Channel<T>,
    mut rate: AdaptiveRate,
    mut make_batch: F,
    mut cancel_rx: oneshot::Receiver<()>,
) where
    T: serde::Serialize,
    F: FnMut() -> Option<T>,
{
    log::info!(
        "{} 订阅已启动, channel_id={}, 初始间隔={}ms",
        name,
        channel_id,
        rate.current().as_millis()
    );
    loop {
        tokio::select! {
            _ = &mut cancel_rx => {
                log::info!("{} 订阅被取消, channel_id={}", name, channel_id);
                break;
            }
            _ = tokio::time::sleep(rate.current()) => {
                match make_batch() {
                    Some(batch) => {
                        if on_event.send(batch).is_err() {
                            log::info!("{} 订阅通道已关闭, channel_id={}", name, channel_id);
                            break;
                        }
                        rate.on_send();
                    }
                    None => rate.on_idle(),
                }
            }
        }
    }
}
