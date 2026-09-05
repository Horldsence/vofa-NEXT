//! 工作区运行控制与后台发送任务注册 Tauri 命令。
//!
//! 层级: L4 cmd — Tauri IPC 薄适配; 切换语义与状态重置都在
//! `data_plane::run_control::apply_run_action`, 本层只做取状态 + 调用 + 事件广播。

use std::collections::HashMap;

use app_state::AppState;
use data_plane::execution::{RunSnapshot, SendMode, SendStatus, SendTask};
use data_plane::run_control::apply_run_action;
use data_plane::{RunAction, RunState};
use error::ConfigError;
use notify_events::emit_workspace_run;
use schema_engine::command_frame::CommandFrameDto;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use vofa_core::{Error, Result};

/// `workspace:run` 事件载荷 — 运行快照 (状态 + epoch)。
/// 前端订阅收敛运行态显示; 命令返回值与事件载荷同构。
#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceRunEvent {
    pub state: RunState,
    pub epoch: u64,
}

impl From<RunSnapshot> for WorkspaceRunEvent {
    fn from(snapshot: RunSnapshot) -> Self {
        Self {
            state: snapshot.state,
            epoch: snapshot.epoch,
        }
    }
}

/// 工作区运行控制 — start (启动/恢复) / pause / stop。
///
/// 幂等: 目标态与当前态相同时不推进 epoch, 原样返回快照。
#[tauri::command]
pub async fn workspace_run(
    app: AppHandle,
    state: State<'_, AppState>,
    action: RunAction,
) -> Result<WorkspaceRunEvent> {
    let (plane, groups) = {
        let s = state.inner();
        (s.data_plane.clone(), s.stream_groups.clone())
    };
    let snapshot = apply_run_action(&plane, &groups, action).await;
    emit_workspace_run(&app, &WorkspaceRunEvent::from(snapshot));
    Ok(snapshot.into())
}

/// 读取当前运行快照 (前端水合用 — 不产生事件)
#[tauri::command]
pub fn get_workspace_run_state(state: State<'_, AppState>) -> Result<WorkspaceRunEvent> {
    Ok(state.data_plane.eval.execution.snapshot().into())
}

/// 前端注册的后台自动发送任务 (Command widget 每个非手动帧一条)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendTaskDto {
    pub widget_id: String,
    pub frame_id: String,
    pub mode: SendMode,
    #[serde(default)]
    pub interval_ms: u64,
    pub frame: CommandFrameDto,
}

/// 注册/替换某 Command widget 的全部自动发送任务 (空列表 = 注销)。
///
/// 前端在帧配置变化与组件卸载时调用 — 发送触发完全在 Rust 侧 ticker,
/// 前端不再持有任何定时器。Manual 模式的帧不入注册表 (手动发送走
/// `send_command_frame` 统一内核)。
#[tauri::command]
pub fn set_widget_send_tasks(
    state: State<'_, AppState>,
    widget_id: String,
    tasks: Vec<SendTaskDto>,
) -> Result<usize> {
    if !state
        .data_plane
        .global_nodes
        .lock()
        .contains_key(&widget_id)
    {
        return Err(Error::Config(ConfigError::NodeNotFound {
            node_id: widget_id,
        }));
    }
    let mut registered: Vec<SendTask> = Vec::with_capacity(tasks.len());
    for task in tasks {
        // 防御: widget_id 以后端参数为准, 载荷内不一致视为非法
        if task.widget_id != widget_id {
            return Err(Error::Config(ConfigError::NodeNotFound {
                node_id: task.widget_id,
            }));
        }
        registered.push(SendTask {
            widget_id: widget_id.clone(),
            frame_id: task.frame_id,
            frame: task.frame,
            mode: task.mode,
            interval_ms: task.interval_ms,
        });
    }
    let count = registered.len();
    state
        .data_plane
        .eval
        .send
        .lock()
        .set_widget_tasks(&widget_id, registered);
    Ok(count)
}

/// 自动发送任务状态表 (task_key → sent/skipped/error) — 调试与状态面板用
#[tauri::command]
pub fn get_send_task_status(state: State<'_, AppState>) -> Result<HashMap<String, SendStatus>> {
    Ok(state.data_plane.eval.send.lock().status_map())
}
