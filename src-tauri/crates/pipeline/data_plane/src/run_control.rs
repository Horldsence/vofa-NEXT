//! 工作区运行控制 — 启动 / 暂停 / 停止的状态机与副作用重置。
//!
//! 切换纪律 (与 [`crate::execution::ExecutionControl::boundary`] 契约一致):
//! 持写锁推进状态 + 完成全部依赖状态重置 — 读侧 (评估批次 / 字节路由 /
//! 发送 IO) 持读锁跨越异步段, 因此切换不可能与在途求值或设备写入交错,
//! 旧 epoch 的异步结果无法在切换后发布。
//!
//! 语义 (对应工作区运行契约):
//! - 暂停: 停止处理与自动发送, 保持设备连接 (读任务存活但丢弃字节),
//!   清空已解析未评估队列 — 不积压暂停期间的数据
//! - 恢复/启动: 建立新流序列 (组序号归零) 并复位全部连续性状态
//! - 停止: 暂停语义 + 清空自动发送任务注册表 (待发送任务一并取消)

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use parking_lot::Mutex;

use crate::eval_state::StreamGroupState;
use crate::execution::{RunAction, RunSnapshot, RunState};
use crate::{graph_eval, DataPlaneState};

/// 流订阅组注册表类型 — AppState.stream_groups 的共享句柄
pub type StreamGroups = Arc<Mutex<HashMap<String, StreamGroupState>>>;

/// 应用一个运行控制动作, 返回切换后的运行快照 (幂等: 目标态相同则原样返回)。
pub async fn apply_run_action(
    plane: &DataPlaneState,
    stream_groups: &StreamGroups,
    action: RunAction,
) -> RunSnapshot {
    let execution = &plane.eval.execution;
    let target = match action {
        RunAction::Start => RunState::Running,
        RunAction::Pause => RunState::Paused,
        RunAction::Stop => RunState::Stopped,
    };
    let current = execution.snapshot();
    if current.state == target {
        return current;
    }

    // 写锁内完成: 状态推进 + 队列/连续性/发送簿记重置 (与在途评估批次互斥)
    let snapshot = {
        let _guard = execution.boundary.write().await;
        let snapshot = execution.transition(target);
        match (current.state, target) {
            // 进入 Paused: 丢弃已解析未评估的积压, 保持连接
            (_, RunState::Paused) => {
                let dropped = plane.clear_all_eval_queues();
                if dropped > 0 {
                    log::debug!("工作区暂停: 丢弃评估队列积压 {dropped} 帧");
                }
                // 发送簿记复位 (任务定义保留): 恢复后 OnChange 重建基线,
                // Timer 从下个周期起算 — 暂停期间的变更不补发
                plane.eval.send.lock().reset_schedules();
            }
            // 进入 Running (启动或恢复): 新流序列 + 全部连续性状态复位
            (_, RunState::Running) => {
                let dropped = plane.clear_all_eval_queues();
                if dropped > 0 {
                    log::debug!("工作区启动: 丢弃切换前评估队列积压 {dropped} 帧");
                }
                for group in stream_groups.lock().values() {
                    group.seq.store(0, Ordering::Release);
                }
                graph_eval::reset_all_transient_state(&plane.eval);
                plane.eval.send.lock().reset_schedules();
            }
            // 进入 Stopped: 暂停语义 + 清空待发送任务
            (_, RunState::Stopped) => {
                let dropped = plane.clear_all_eval_queues();
                if dropped > 0 {
                    log::debug!("工作区停止: 丢弃评估队列积压 {dropped} 帧");
                }
                let mut send = plane.eval.send.lock();
                send.reset_schedules();
                send.clear();
            }
        }
        snapshot
    };
    log::info!(
        "工作区运行状态: {:?} -> {:?} (epoch {})",
        current.state,
        snapshot.state,
        snapshot.epoch
    );
    snapshot
}
