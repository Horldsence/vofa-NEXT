//! 评估 worker — 数值平面独立任务 (摄入/评估解耦点)
//!
//! 字节平面 (读任务) 只负责解析, 产帧入每源有界帧队列; 本 worker 单实例
//! 消费所有队列, 经 [`frame_dispatch::on_frames`] 完成缓冲推送与图评估。
//!
//! 设计约束:
//! - **单 worker**: 图求值状态 (滤波/触发/解码/source_frames) 跨源共享, 串行化
//!   保持旧同步调用 (读任务内联 eval) 的批间原子语义
//! - **公平轮询**: 每次取批在不同非空源间轮转, 快源不饿死慢源
//! - **有界 + 丢最旧**: 队列满时丢最旧整批并计数 (`eval_dropped`) —
//!   持续过载下丢最旧保最新, 波形尾部始终可见, 且丢弃显式可观测
//!   (取代解耦前 broadcast Lagged 在评估段的等价物)
//! - **重求值不占 tokio worker**: 批量求值 (SIMD/并行, 见
//!   [`frame_dispatch::on_frames`]) 经 blocking 池执行, worker 任务只做调度

use super::DataPlaneState;

/// 每源队列深度 (批数)。8 批 × ~12k 帧/批 ≈ 10 万帧缓冲 (~400KB/源 @4 通道),
/// 足以吸收评估段的秒级毛刺, 同时限制最坏内存与评估延迟。
pub(super) const EVAL_QUEUE_DEPTH: usize = 8;

/// 评估 worker 主循环 — 随首个 attach 启动, 常驻
pub(super) async fn eval_worker(plane: DataPlaneState) {
    log::debug!("评估 worker 已启动");
    let notify = plane.eval_notify.clone();
    loop {
        // Notify 许可语义保证不丢唤醒: 生产者 push 后 notify_one, 即使 worker
        // 尚未进入等待, 许可也会存储并在下次 notified() 立即返回
        notify.notified().await;
        while let Some((source, frames)) = plane.pop_frame_batch() {
            let frame_count = frames.len();
            let eval_plane = plane.clone();
            let eval_task = tokio::task::spawn_blocking(move || {
                super::frame_dispatch::on_frames(&eval_plane, &source, &frames)
            });
            match eval_task.await {
                Ok(eval_ns) => {
                    plane
                        .metrics
                        .eval_ns
                        .fetch_add(eval_ns, std::sync::atomic::Ordering::Relaxed);
                    plane.metrics.frames_evaled.fetch_add(
                        u64::try_from(frame_count).unwrap_or(u64::MAX),
                        std::sync::atomic::Ordering::Relaxed,
                    );
                }
                // panic 语义透传 (求值 bug 不应被静默吞掉), 关闭型 JoinError 忽略
                Err(e) if e.is_panic() => std::panic::resume_unwind(e.into_panic()),
                Err(_) => {}
            }
        }
    }
}

/// 清空某 Transport 下游所有 Protocol 节点的评估队列 (TestData 停止边沿用),
/// 返回丢弃帧数。语义与"排空广播积压"一致: 停止后波形立即冻结, 不拖尾。
pub(super) fn clear_downstream_queues(plane: &DataPlaneState, transport_id: &str) -> u64 {
    use kind::NodeKind;

    let mut dropped = 0_u64;
    let mut pending = std::collections::VecDeque::from([transport_id.to_string()]);
    let mut visited = std::collections::HashSet::new();
    let mut targets = Vec::new();
    {
        let plan = plane.byte_plan.lock();
        let nodes = plane.global_nodes.lock();
        while let Some(source) = pending.pop_front() {
            if !visited.insert(source.clone()) {
                continue;
            }
            for route in plan.routes_for(&source) {
                if matches!(
                    nodes.get(&route.target).map(|node| &node.kind),
                    Some(NodeKind::Protocol { .. })
                ) {
                    targets.push(route.target.clone());
                }
                pending.push_back(route.target.clone());
            }
        }
    }
    if !targets.is_empty() {
        let mut queues = plane.frame_queues.lock();
        for target in targets {
            if let Some(queue) = queues.get_mut(&target) {
                dropped += queue.drain(..).map(|batch| batch.len() as u64).sum::<u64>();
            }
        }
    }
    dropped
}
