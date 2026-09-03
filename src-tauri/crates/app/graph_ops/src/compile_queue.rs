//! 图编译队列 — per-tab last-write-wins, 编译后台执行, 状态事件广播.
//!
//! 拆分方式:
//! - `CompileQueue` 仅维护 pending 与每 tab 的"是否在编译中"标志 (轻状态, 不持有 AppState)
//! - 实际编译由 [`graph::graph::apply_tab_graph`] 提供, `update_tab_graph` 调用方负责:
//!   1. `queue.submit()` — 立即写 pending 并返回 receipt, 不阻塞 IPC
//!   2. `tokio::spawn` 包裹的 worker 协程, 通过 `queue.try_take(tab_id, my_seq)` 判断
//!      自己是否仍是最新; 是则执行 compile + commit + emit, 不是则直接返回

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use buffer_graph::Edge;
use kind::NodeDef;
use parking_lot::Mutex as PMutex;
use serde::Serialize;

/// 编译状态
#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompileState {
    #[default]
    Idle,
    Pending,
    Compiling,
    Ok,
    Error,
}

/// 单次待编译请求
pub struct PendingRequest {
    pub nodes: Vec<NodeDef>,
    pub edges: Vec<Edge>,
    /// 单调递增 seq — worker 用此判断自己是否过期
    pub seq: u64,
}

/// `submit()` 返回的回执 — 前端可立刻拿到 queued_seq
#[derive(Debug, Clone, Serialize)]
pub struct Receipt {
    pub tab_id: String,
    pub queued_seq: u64,
}

/// `graph:compile` 事件 payload — 编译队列对外状态广播
///
/// 字段:
/// - `tab_id` — 受影响 tab id
/// - `state` — 当前状态 (`pending` / `compiling` / `ok` / `error`)
/// - `queued_seq` — 该结果对应的提交 seq (`Receipt::queued_seq`)
/// - `report` — 错误详情 (`state = error` 时填充; `state = ok` 时为 `None`)
#[derive(Debug, Clone, Serialize)]
pub struct GraphCompileEvent {
    pub tab_id: String,
    pub state: CompileState,
    pub queued_seq: u64,
    pub report: Option<error::CompileReport>,
}

/// 队列内部状态
struct QueueState {
    /// 等待编译的最新 pending (last-write-wins, 同 tab 后到的覆盖先到的)
    pending: HashMap<String, PendingRequest>,
    /// 该 tab 是否有 worker 已在运行
    compiling: HashSet<String>,
    /// 全局 seq 计数器
    next_seq: u64,
    /// 已发布的 receipt_seq (用于 stale 校验)
    published: HashMap<String, u64>,
    /// 全局 receipt_seq 计数器 — 跨 tab 单调, 前端 stale-check 使用
    receipt_seq: AtomicU64,
    /// 最近一次发布状态 (供 `current_event` 查询 / 测试用)
    last_state: HashMap<String, CompileState>,
}

impl Default for QueueState {
    fn default() -> Self {
        Self {
            pending: HashMap::new(),
            compiling: HashSet::new(),
            next_seq: 1,
            published: HashMap::new(),
            receipt_seq: AtomicU64::new(0),
            last_state: HashMap::new(),
        }
    }
}

/// 编译队列
pub struct CompileQueue {
    state: PMutex<QueueState>,
}

impl Default for CompileQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl CompileQueue {
    pub fn new() -> Self {
        Self {
            state: PMutex::new(QueueState::default()),
        }
    }

    /// 写入 pending, 标记 worker 启动职责, 返回 receipt
    ///
    /// 返回 `need_spawn` 决定调用方是否需要 `tokio::spawn` 一个 worker.
    /// 同 tab 若已有 worker, 调用方应跳过 spawn, worker 在循环末尾会自动拾取新 pending.
    pub fn submit(&self, tab_id: String, nodes: Vec<NodeDef>, edges: Vec<Edge>) -> SubmitResult {
        let mut state = self.state.lock();
        let seq = state.next_seq;
        state.next_seq = state.next_seq.wrapping_add(1);
        let is_new = !state.pending.contains_key(&tab_id);
        state
            .pending
            .insert(tab_id.clone(), PendingRequest { nodes, edges, seq });
        let need_spawn = if state.compiling.contains(&tab_id) {
            false
        } else {
            state.compiling.insert(tab_id.clone());
            true
        };
        SubmitResult {
            receipt: Receipt {
                tab_id,
                queued_seq: seq,
            },
            need_spawn,
            is_new,
        }
    }

    /// Worker 启动入口 — 由 `update_tab_graph` 在 spawn 后调用.
    /// 若本调用点已不是最新请求, 返回 `false`, worker 应直接退出.
    pub fn try_take(&self, tab_id: &str, my_seq: u64) -> bool {
        let mut state = self.state.lock();
        let Some(pending) = state.pending.get(tab_id) else {
            state.compiling.remove(tab_id);
            return false;
        };
        if pending.seq != my_seq {
            // 已有更新的 pending, 当前 worker 把 compiling 让给后续 worker
            return false;
        }
        true
    }

    /// Worker 完成时清空 (顺手释放 compiling 标记)
    pub fn finish(&self, tab_id: &str, my_seq: u64) {
        let mut state = self.state.lock();
        // 仅清空最新 pending — 中间到来的更新仍保留
        let still_mine = state.pending.get(tab_id).is_some_and(|p| p.seq == my_seq);
        if still_mine {
            let receipt = state.receipt_seq.fetch_add(1, Ordering::Relaxed) + 1;
            state.published.insert(tab_id.to_string(), receipt);
            state
                .last_state
                .insert(tab_id.to_string(), CompileState::Ok);
        }
        state.compiling.remove(tab_id);
    }

    /// Worker 完成时记录错误状态
    pub fn finish_error(&self, tab_id: &str, my_seq: u64) {
        let mut state = self.state.lock();
        let still_mine = state.pending.get(tab_id).is_some_and(|p| p.seq == my_seq);
        if still_mine {
            let receipt = state.receipt_seq.fetch_add(1, Ordering::Relaxed) + 1;
            state.published.insert(tab_id.to_string(), receipt);
            state
                .last_state
                .insert(tab_id.to_string(), CompileState::Error);
        }
        state.compiling.remove(tab_id);
    }

    /// 仍需继续处理的 pending (worker 完成后, 检查是否有更新的请求等待下次循环)
    pub fn has_pending(&self) -> Vec<String> {
        let state = self.state.lock();
        state.pending.keys().cloned().collect()
    }

    /// 最近一次发布的状态 (测试 / 状态栏查询)
    pub fn current_state(&self, tab_id: &str) -> CompileState {
        let state = self.state.lock();
        state
            .last_state
            .get(tab_id)
            .copied()
            .unwrap_or(CompileState::Idle)
    }
}

/// `submit()` 的返回 — 同时携带是否需要 spawn worker
pub struct SubmitResult {
    pub receipt: Receipt,
    pub need_spawn: bool,
    /// 该 tab 是否之前从未提交过 (前端用它避免重复清空派生表)
    pub is_new: bool,
}

/// 进程级单例 — Tauri 命令调用入口 (无需修改 AppState, 避免 graph / app_state 循环)
///
/// 该单例被所有 Tauri 命令隐式共享; 单测绕过该单例, 直接构造自己的 `CompileQueue`.
static GLOBAL_QUEUE: OnceLock<Arc<CompileQueue>> = OnceLock::new();

/// 获取（或初始化）全局队列 — 调用方应缓存返回的 `Arc`
pub fn global() -> Arc<CompileQueue> {
    GLOBAL_QUEUE
        .get_or_init(|| Arc::new(CompileQueue::new()))
        .clone()
}

/// 测试辅助 — 重置全局单例 (受测试夹具控制)
#[cfg(test)]
pub fn reset_for_test() {
    if let Some(q) = GLOBAL_QUEUE.get() {
        // 仅清空内部状态, 不替换 Arc (避免悬挂 weak ref)
        let _ = q;
    }
}
