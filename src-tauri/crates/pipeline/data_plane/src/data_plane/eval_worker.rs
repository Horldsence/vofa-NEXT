//! 评估 worker — 数值平面独立任务 (摄入/评估解耦点)
//!
//! 字节平面 (读任务) 负责解析 + **记录平面入库** (record_frames, 原始波形
//! 不依赖本 worker); 本 worker 单实例消费评估队列, 经
//! [`frame_dispatch::eval_frames`] 完成 source_frames 更新与图评估,
//! 派生输出写独立时间轴。
//!
//! 设计约束:
//! - **单 worker**: 图求值状态 (滤波/触发/解码/source_frames) 跨源共享, 串行化
//!   保持旧同步调用 (读任务内联 eval) 的批间原子语义
//! - **公平轮询**: 每次取批在不同非空源间轮转, 快源不饿死慢源
//! - **有界 + 丢最旧 + 显式缺口**: 队列满时丢最旧整批并计数 (`eval_dropped`);
//!   取批时若该源有缺口, 先复位关联有状态算子 (滤波/触发/IFFT) 并告警 —
//!   绝不带断裂状态产出看似连续的近似值 (不变量 5)
//! - **重求值不占 tokio worker**: 批量求值 (SIMD/并行, 见
//!   [`frame_dispatch::on_frames`]) 经 blocking 池执行, worker 任务只做调度

use super::DataPlaneState;

/// 评估 worker 主循环 — 随首个 attach 启动, 常驻
pub(super) async fn eval_worker(plane: DataPlaneState) {
    log::debug!("评估 worker 已启动");
    let notify = plane.eval_notify.clone();
    loop {
        // Notify 许可语义保证不丢唤醒: 生产者 push 后 notify_one, 即使 worker
        // 尚未进入等待, 许可也会存储并在下次 notified() 立即返回
        notify.notified().await;
        while let Some((source, frames, gap)) = plane.pop_frame_batch() {
            // boundary 读锁先行并跨越整个批次求值: 运行状态切换 (写锁) 等待
            // 在途批次完成 — 旧 epoch 的求值结果不可能在切换后发布。
            let _boundary = plane.eval.execution.boundary.read().await;
            // 运行门控: 暂停/停止瞬间残留入队的批次直接丢弃 (切换路径已清空
            // 队列, 此处兜底竞态窗口内入队的最后一批), 不在非运行态求值。
            // 锁内复查 — 取锁与查票之间不会有切换插入。
            if plane.eval.execution.ticket().is_none() {
                continue;
            }
            // 缺口 (队列溢出丢批) → 复位该源关联有状态算子 (不变量 5)
            if gap {
                crate::graph_eval::reset_source_transient_state(&plane.eval, &source);
            }
            let frame_count = frames.len();
            let eval_plane = plane.clone();
            let dispatched = std::time::Instant::now();
            let eval_task = tokio::task::spawn_blocking(move || {
                eval_plane.metrics.dispatch_wait_max_ns.fetch_max(
                    u64::try_from(dispatched.elapsed().as_nanos()).unwrap_or(u64::MAX),
                    std::sync::atomic::Ordering::Relaxed,
                );
                let service = std::time::Instant::now();
                let buffer = eval_plane.buffer_for(&source);
                let options = super::frame_dispatch::EvalOptions::from_config(
                    &eval_plane.pipeline_config.read(),
                );
                let eval_ns = super::frame_dispatch::eval_frames(
                    &eval_plane.eval,
                    &eval_plane.global_nodes,
                    &buffer,
                    &source,
                    &frames,
                    options,
                );
                eval_plane.metrics.eval_service_max_ns.fetch_max(
                    u64::try_from(service.elapsed().as_nanos()).unwrap_or(u64::MAX),
                    std::sync::atomic::Ordering::Relaxed,
                );
                eval_ns
            });
            match eval_task.await {
                Ok(eval_ns) => {
                    plane
                        .metrics
                        .eval_batches_total
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    plane
                        .metrics
                        .eval_completed_total
                        .fetch_add(frame_count as u64, std::sync::atomic::Ordering::Relaxed);
                    plane
                        .metrics
                        .eval_batches
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
