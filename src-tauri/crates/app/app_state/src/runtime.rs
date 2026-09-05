//! 运行时装配 — 二进制组合根 (vofa-next lib.rs) 调用的后台任务启动与退出 flush
//!
//! 层级: L3 app。把工作区防抖落盘、3 个推送 ticker、启动页兜底等进程级
//! 后台任务集中在此; 二进制只剩插件装配与命令注册。

use std::path::PathBuf;
use tauri::{AppHandle, Manager};

use crate::app_state::AppState;
use crate::{collect_workspace_file, save_workspace};
use crate::{send_scheduler_ticker, spectrum_ticker, text_output_ticker, textout_sender_ticker};

/// setup 阶段启动全部后台任务: 工作区防抖落盘 / 文本输出 & TextOut & 频谱 ticker /
/// 启动页兜底。
///
/// `config_dir`: workspace.json 所在目录 (app config dir)。
pub fn spawn_background_tasks(app: &AppHandle, config_dir: PathBuf) {
    spawn_workspace_autosave(app, config_dir);
    spawn_push_tickers(app);
    spawn_splashscreen_fallback(app);
}

/// 工作区防抖落盘任务 — 图提交 / 位置上报 / tab 变更置 dirty,
/// 此任务周期性检查并整体覆盖写 (800ms 合并连发编辑)
fn spawn_workspace_autosave(app: &AppHandle, dir: PathBuf) {
    let (ws, graphs) = {
        let state = app.state::<AppState>();
        (
            state.workspace.clone(),
            std::sync::Arc::clone(&state.source_graphs),
        )
    };
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
            let should_save = {
                let mut w = ws.lock();
                if w.dirty {
                    w.dirty = false;
                    true
                } else {
                    false
                }
            };
            if should_save {
                let file = collect_workspace_file(&ws, &graphs);
                if let Err(e) = save_workspace(&dir, &file) {
                    log::warn!("工作区落盘失败: {e}");
                }
            }
        }
    });
}

/// 启动字符串输出 (30 FPS) / TextOut 发送 / 频谱分析三个推送 ticker
fn spawn_push_tickers(app: &AppHandle) {
    // 启动字符串输出 ticker (30 FPS 推送给 TextDisplay)
    let eval_state_for_text = app.state::<AppState>().eval_state();
    tauri::async_runtime::spawn(text_output_ticker(eval_state_for_text));

    // 启动 TextOut 发送 ticker (图内字符串限速写回目标 Transport 的 tx)
    let (eval_state_for_textout, transport_for_textout) = {
        let state = app.state::<AppState>();
        (
            state.eval_state(),
            std::sync::Arc::clone(&state.data_plane.transport),
        )
    };
    tauri::async_runtime::spawn(textout_sender_ticker(
        eval_state_for_textout,
        transport_for_textout,
    ));

    // 启动频谱分析 ticker (30 FFT 计算 + 推送 SpectrumBatch)
    let eval_state_for_spectrum = app.state::<AppState>().eval_state();
    tauri::async_runtime::spawn(spectrum_ticker(eval_state_for_spectrum));

    // 启动后台自动发送调度 ticker (Timer/OnChange; 手动发送走统一内核命令)
    let (app_handle_for_send, plane_for_send, graphs_for_send) = {
        let state = app.state::<AppState>();
        (
            app.clone(),
            state.data_plane.clone(),
            std::sync::Arc::clone(&state.source_graphs),
        )
    };
    tauri::async_runtime::spawn(send_scheduler_ticker(
        app_handle_for_send,
        plane_for_send,
        graphs_for_send,
    ));
}

/// 启动页兜底: 前端应在初始化完成后调用 close_splashscreen 关闭启动页;
/// 若前端异常迟迟未调用, 超时强制切换, 防止永远卡在启动页
fn spawn_splashscreen_fallback(app: &AppHandle) {
    let fallback_handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(10));
        if let Some(splash) = fallback_handle.get_webview_window("splashscreen") {
            log::warn!("splashscreen fallback: force closing after timeout");
            let _ = splash.close();
        }
        if let Some(main) = fallback_handle.get_webview_window("main") {
            let _ = main.show();
            let _ = main.set_focus();
        }
    });
}

/// 退出 flush: 防抖任务最长 800ms 间隔, 正常退出前把未落盘的
/// 工作区变更同步写盘, 避免丢最后一次编辑
pub fn flush_workspace_on_exit(app: &AppHandle) {
    let state = app.state::<AppState>();
    let ws = &state.workspace;
    if ws.lock().dirty {
        let file = collect_workspace_file(ws, &state.source_graphs);
        let dir = app
            .path()
            .app_config_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."));
        if let Err(e) = save_workspace(&dir, &file) {
            log::warn!("工作区退出落盘失败: {e}");
        }
    }
}
