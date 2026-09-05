//! 工作区生命周期、确定性有界发送调度与运行门控。
//!
//! [`ExecutionControl`] 是运行状态的唯一权威: 状态 (2 bit) + epoch 打包进一个
//! 原子字, 读侧永远看不到撕裂的票据。所有产出/消费数据链副作用方 (读任务路由、
//! 评估批次、发送调度) 持 [`ExecutionControl::boundary`] 读锁跨越其异步段,
//! 运行状态切换持写锁 — 切换因此与在途求值批次/设备写入互斥, 旧 epoch 的
//! 异步结果不可能在切换后发布或发送。
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use schema_engine::command_frame::CommandFrameDto;

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    #[default]
    Stopped,
    Running,
    Paused,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct RunSnapshot {
    pub state: RunState,
    pub epoch: u64,
}

/// 运行控制动作 — Start 从 Stopped 启动 / 从 Paused 恢复 (语义相同: 重建流序列)。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunAction {
    Start,
    Pause,
    Stop,
}

/// State and epoch occupy one atomic word, so readers cannot observe torn tickets.
#[derive(Default)]
pub struct ExecutionControl {
    word: AtomicU64,
    /// 互斥域: 读侧 (评估批次 / 发送 IO / 字节路由) 持读锁, 运行状态切换持写锁。
    /// 用 tokio 的异步感知实现 — 读 guard 需要跨越 await。
    pub boundary: tokio::sync::RwLock<()>,
}

impl ExecutionControl {
    pub fn snapshot(&self) -> RunSnapshot {
        let word = self.word.load(Ordering::Acquire);
        RunSnapshot {
            state: match word & 3 {
                1 => RunState::Running,
                2 => RunState::Paused,
                _ => RunState::Stopped,
            },
            epoch: word >> 2,
        }
    }

    /// 运行中返回当前原子字作为票据; 暂停/停止返回 None (数据链门控依据)。
    pub fn ticket(&self) -> Option<u64> {
        let word = self.word.load(Ordering::Acquire);
        (word & 3 == 1).then_some(word)
    }

    /// 票据是否仍然有效 (epoch 未前进) — 异步结果发布/发送前的最后校验。
    pub fn accepts(&self, ticket: u64) -> bool {
        ticket & 3 == 1 && self.word.load(Ordering::Acquire) == ticket
    }

    /// 切换运行状态并推进 epoch。
    ///
    /// 调用方必须持 [`ExecutionControl::boundary`] 写锁, 并在锁内完成依赖状态的
    /// 重置 (评估队列 / 连续性状态 / 发送基线) — 保证切换原子地对在途批次生效。
    pub fn transition(&self, state: RunState) -> RunSnapshot {
        let epoch = self.snapshot().epoch.wrapping_add(1);
        let tag = match state {
            RunState::Stopped => 0,
            RunState::Running => 1,
            RunState::Paused => 2,
        };
        self.word.store((epoch << 2) | tag, Ordering::Release);
        self.snapshot()
    }
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SendMode {
    #[default]
    Manual,
    Timer,
    OnChange,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct SendStatus {
    pub sent: u64,
    pub skipped: u64,
    pub error: Option<String>,
}

/// Stores only the last successfully sent payload; pending changes are coalesced
/// by observing the latest encoded value. Time is supplied by the caller for tests.
#[derive(Debug, Default, Clone)]
pub struct SendSchedule {
    baseline: Option<Vec<u8>>,
    deadline_ms: u64,
    pub status: SendStatus,
}

impl SendSchedule {
    pub fn reset(&mut self) {
        self.baseline = None;
        self.deadline_ms = 0;
    }

    pub fn due(
        &mut self,
        now_ms: u64,
        mode: SendMode,
        interval_ms: u64,
        payload: Option<&[u8]>,
    ) -> bool {
        let Some(payload) = payload.filter(|p| !p.is_empty()) else {
            self.reset();
            return false;
        };
        let interval_ms = interval_ms.max(1);
        let Some(baseline) = &self.baseline else {
            self.baseline = Some(payload.to_vec());
            self.deadline_ms = now_ms.saturating_add(interval_ms);
            return false;
        };
        if mode == SendMode::Manual || now_ms < self.deadline_ms {
            return false;
        }
        if mode == SendMode::OnChange && baseline == payload {
            return false;
        }
        if mode == SendMode::Timer {
            self.status.skipped = self
                .status
                .skipped
                .saturating_add((now_ms - self.deadline_ms) / interval_ms);
        }
        self.deadline_ms = now_ms.saturating_add(interval_ms);
        true
    }

    pub fn complete(&mut self, payload: &[u8], result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.baseline
                    .get_or_insert_with(Vec::new)
                    .clone_from(&payload.to_vec());
                self.status.sent = self.status.sent.saturating_add(1);
                self.status.error = None;
            }
            Err(error) => {
                self.reset();
                self.status.error = Some(error);
            }
        }
    }
}

/// 一条后台自动发送任务 — Command widget 的一个非手动帧。
///
/// `widget_id` 既是字节路由源 (沿 `loopbackOut` 字节边下发), 也是 var_ref
/// 输入值的解析锚点 (源图边 target == widget_id)。
#[derive(Debug, Clone)]
pub struct SendTask {
    pub widget_id: String,
    pub frame_id: String,
    pub frame: CommandFrameDto,
    pub mode: SendMode,
    pub interval_ms: u64,
}

impl SendTask {
    pub fn task_key(widget_id: &str, frame_id: &str) -> String {
        format!("{widget_id}\u{0}{frame_id}")
    }

    pub fn key(&self) -> String {
        Self::task_key(&self.widget_id, &self.frame_id)
    }
}

/// 单任务调度状态: 任务定义 + 确定性调度簿记。
#[derive(Debug, Clone)]
pub struct SendTaskState {
    pub task: SendTask,
    pub schedule: SendSchedule,
}

/// 后台自动发送任务注册表 — 前端经 `set_widget_send_tasks` 按 widget 全量替换,
/// 调度 ticker 逐 tick 消费。Manual 模式不注册 (手动发送走统一内核命令)。
#[derive(Default)]
pub struct SendScheduler {
    tasks: HashMap<String, SendTaskState>,
}

impl SendScheduler {
    /// 替换某 widget 的全部任务 (幂等; 空列表 = 注销该 widget)。
    /// 已存在同 key 任务保留调度簿记 (OnChange 基线不因无关编辑重建)。
    pub fn set_widget_tasks(&mut self, widget_id: &str, tasks: Vec<SendTask>) {
        self.tasks.retain(|_, st| st.task.widget_id != widget_id);
        for task in tasks {
            if task.mode == SendMode::Manual {
                continue;
            }
            self.tasks.insert(
                task.key(),
                SendTaskState {
                    schedule: SendSchedule::default(),
                    task,
                },
            );
        }
    }

    /// 注销某 widget 的全部任务。
    pub fn remove_widget(&mut self, widget_id: &str) {
        self.tasks.retain(|_, st| st.task.widget_id != widget_id);
    }

    /// 清空调度簿记 (任务定义保留) — 暂停/恢复后 OnChange 基线需要重建。
    pub fn reset_schedules(&mut self) {
        for st in self.tasks.values_mut() {
            st.schedule.reset();
        }
    }

    /// 全量清空 — 停止时调用 (执行状态与待发送任务一并清除)。
    pub fn clear(&mut self) {
        self.tasks.clear();
    }

    /// 剔除 widget 已不存在的任务 (图删除/卸载后遗留)。
    pub fn retain_existing(&mut self, mut exists: impl FnMut(&str) -> bool) {
        self.tasks.retain(|_, st| exists(&st.task.widget_id));
    }

    pub fn tasks_mut(&mut self) -> impl Iterator<Item = &mut SendTaskState> {
        self.tasks.values_mut()
    }

    /// 按任务键取单个任务状态 (调度簿记回写用)。
    pub fn task_mut(&mut self, key: &str) -> Option<&mut SendTaskState> {
        self.tasks.get_mut(key)
    }

    pub fn status_map(&self) -> HashMap<String, SendStatus> {
        self.tasks
            .iter()
            .map(|(k, st)| (k.clone(), st.schedule.status.clone()))
            .collect()
    }

    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_coalesce_and_first_value_only_arms() {
        let mut schedule = SendSchedule::default();
        assert!(!schedule.due(0, SendMode::OnChange, 50, Some(b"A")));
        assert!(!schedule.due(20, SendMode::OnChange, 50, Some(b"B")));
        assert!(schedule.due(50, SendMode::OnChange, 50, Some(b"C")));
        schedule.complete(b"C", Ok(()));
        assert!(!schedule.due(100, SendMode::OnChange, 50, Some(b"C")));
        assert_eq!(schedule.status.sent, 1);
    }

    #[test]
    fn missed_timer_periods_are_skipped_and_failures_do_not_replay() {
        let mut schedule = SendSchedule::default();
        assert!(!schedule.due(0, SendMode::Timer, 100, Some(b"A")));
        assert!(schedule.due(350, SendMode::Timer, 100, Some(b"A")));
        assert_eq!(schedule.status.skipped, 2);
        schedule.complete(b"A", Err("disconnected".into()));
        assert!(!schedule.due(400, SendMode::Timer, 100, None));
        assert!(!schedule.due(500, SendMode::Timer, 100, Some(b"B")));
        assert!(!schedule.due(599, SendMode::Timer, 100, Some(b"B")));
        assert!(schedule.due(600, SendMode::Timer, 100, Some(b"B")));
    }

    #[test]
    fn encoding_failure_invalidates_previous_bytes() {
        let mut schedule = SendSchedule::default();
        schedule.due(0, SendMode::OnChange, 1, Some(b"A"));
        assert!(!schedule.due(1, SendMode::OnChange, 1, None));
        assert!(!schedule.due(2, SendMode::OnChange, 1, Some(b"B")));
    }

    #[test]
    fn lifecycle_tickets_cannot_survive_restart() {
        let control = ExecutionControl::default();
        assert!(control.ticket().is_none());
        control.transition(RunState::Running);
        let ticket = control.ticket().unwrap();
        control.transition(RunState::Paused);
        control.transition(RunState::Running);
        assert!(!control.accepts(ticket));
    }

    fn frame_task(widget_id: &str, frame_id: &str, mode: SendMode) -> SendTask {
        SendTask {
            widget_id: widget_id.to_string(),
            frame_id: frame_id.to_string(),
            frame: CommandFrameDto {
                blocks: Vec::new(),
                append_newline: false,
            },
            mode,
            interval_ms: 100,
        }
    }

    #[test]
    fn widget_task_replacement_preserves_other_widgets() {
        let mut scheduler = SendScheduler::default();
        scheduler.set_widget_tasks("w1", vec![frame_task("w1", "f1", SendMode::Timer)]);
        scheduler.set_widget_tasks("w2", vec![frame_task("w2", "f1", SendMode::OnChange)]);
        assert_eq!(scheduler.len(), 2);

        // 同 widget 重挂 (编辑/StrictMode 双挂) — 键集合不增长
        scheduler.set_widget_tasks("w1", vec![frame_task("w1", "f1", SendMode::Timer)]);
        assert_eq!(scheduler.len(), 2);

        // 手动模式不入注册表
        scheduler.set_widget_tasks("w3", vec![frame_task("w3", "f1", SendMode::Manual)]);
        assert_eq!(scheduler.len(), 2);

        // 空列表 = 注销
        scheduler.set_widget_tasks("w1", Vec::new());
        assert_eq!(scheduler.len(), 1);
    }

    #[test]
    fn prune_and_status_surface() {
        let mut scheduler = SendScheduler::default();
        scheduler.set_widget_tasks("w1", vec![frame_task("w1", "f1", SendMode::Timer)]);
        scheduler.retain_existing(|id| id != "w1");
        assert!(scheduler.is_empty());
        scheduler.set_widget_tasks("w2", vec![frame_task("w2", "f1", SendMode::OnChange)]);
        assert!(scheduler.status_map().contains_key("w2\u{0}f1"));
    }
}
