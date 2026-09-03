//! `vofa-next` 二进制组合根 — 只做插件装配、状态管理与命令注册。
//!
//! 层级: L5 (顶端)。全部业务逻辑位于 `crates/<层>/*`; 后台任务 (ticker /
//! 工作区防抖落盘 / 启动页兜底) 在 [`app_state`] 的 `runtime` 模块。

// 必须 glob 导入: `#[tauri::command]` 生成的 `__cmd__*` / `__tauri_command_name_*`
// 宏 (`#[macro_export]`, doc-hidden) 只在 glob 导入时进入本 crate 宏命名空间,
// `generate_handler!` 展开时需要它们在作用域内。
pub use ai::*;
pub use buffer::*;
pub use can_load::*;
pub use can_transport::*;
pub use debug::*;
pub use display::*;
pub use graph::*;
pub use pipeline::*;
pub use rawdata::*;

pub use app_state::AppState;
pub use menu_shell::{build_menu, ids, on_menu_event};
pub use update_flow::*;

use tauri::Manager;
use tauri_plugin_log::{Target, TargetKind};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                // 日志级别: 开发构建 (debug_assertions) 显示 debug 级诊断日志,
                // 发布构建只显示 info 及以上
                .level(if cfg!(debug_assertions) {
                    log::LevelFilter::Debug
                } else {
                    log::LevelFilter::Info
                })
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: None }),
                    Target::new(TargetKind::Webview),
                ])
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(AppState::new())
        .manage(PendingUpdate(Default::default()))
        .setup(|app| {
            // 构建原生菜单栏。
            // - macOS: 菜单位于系统全局菜单栏, 与窗口透明无关, 正常挂载。
            // - Linux: GTK 菜单栏自带不透明背景, 无此问题, 正常挂载。
            // - Windows: 主窗口为透明窗口 (WS_EX_LAYERED) 时, 原生菜单栏无法正确绘制:
            //   菜单文字按深色模式渲染为白色, 但菜单背景在分层窗口上不填充, 造成
            //   "白字白底" 且背景透视露出下层内容。因此 Windows 仅构建但不挂载原生菜单,
            //   改由前端自定义菜单栏 (MenuBar.tsx) 承担同等功能。
            let menu = build_menu(app)?;
            #[cfg(not(target_os = "windows"))]
            app.set_menu(menu)?;

            // AI 状态: MCP server 配置持久化在 app config dir
            let ai_config_dir = app
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            app.manage(ai::AiState::new(ai_config_dir.clone()));

            // 工作区启动恢复: widget 配置 + 画布位置 + tab 元数据 + 各 tab 源图
            // 落盘在 app config dir 的 workspace.json; 恢复后逐 tab 重编译,
            // 前端就绪后经 workspace_get 水合 (返回 false = 无持久化, 默认启动)
            let restored = tauri::async_runtime::block_on(async {
                let state = app.state::<AppState>();
                graph::restore_workspace(&state, &ai_config_dir).await
            });
            log::info!(
                "workspace restore: {}",
                if restored { "loaded" } else { "none" }
            );

            // 后台任务: 工作区防抖落盘 / 推送 ticker / 启动页兜底
            app_state::spawn_background_tasks(app.handle(), ai_config_dir);

            Ok(())
        })
        .on_menu_event(|app, event| on_menu_event(app, event.id().as_ref()))
        .invoke_handler(tauri::generate_handler![
            // 传输
            list_ports,
            open_transport,
            close_transport,
            send_raw,
            send_string,
            send_text_out_now,
            send_widget_value,
            send_and_capture,
            get_connection_state,
            get_stats,
            start_test_data,
            stop_test_data,
            get_test_data_state,
            update_transport_protocol,
            // 协议
            set_protocol,
            get_protocol,
            get_detected_channels,
            // 流水线参数
            set_pipeline_config,
            get_pipeline_config,
            // 波形缓冲区
            subscribe_data,
            unsubscribe_data,
            ack_data,
            get_data_health,
            get_recent_waveform,
            get_waveform_window,
            clear_buffer,
            set_buffer_channels,
            get_buffer_info,
            set_waveform_buffer_capacity,
            set_rawdata_buffer_capacity,
            set_can_buffer_capacity,
            set_logic_buffer_capacity,
            // 节点图 (后端化重构)
            update_tab_graph,
            get_graph_hir,
            remove_tab_graph,
            connect_edge,
            disconnect_edge,
            get_source_graph,
            set_input_value,
            set_node_positions,
            workspace_get,
            workspace_set_tabs,
            submit_custom_output,
            submit_custom_text_output,
            inject_bytes,
            // 原始数据
            clear_raw_data_collector,
            // CAN 帧
            send_can_frame,
            get_recent_can_frames,
            clear_can_buffer,
            get_can_buffer_info,
            list_candle_devices,
            // 逻辑分析仪
            get_recent_logic_samples,
            clear_logic_buffer,
            get_logic_buffer_info,
            get_recent_decoded_events,
            clear_decoded_buffer,
            get_decoded_buffer_info,
            // CAN 负载分析
            get_can_load_stats,
            set_can_load_window,
            clear_can_load_stats,
            get_current_can_bitrate,
            export_can_load_csv,
            // 帧解码器手动测试 (FrameDecoder 面板)
            parse_frame_decoder_input,
            // 触发器匹配 (Trigger 面板手动 / 自动模式)
            match_trigger_command,
            // 命令帧字节打包 (CommandSender 发送路径)
            compute_command_frame_bytes,
            // 调试
            inspect_element,
            // 窗口
            set_window_acrylic,
            close_splashscreen,
            // 应用更新
            check_update,
            download_and_install_update,
            // AI 对话 + MCP
            ai_list_providers,
            ai_chat_send,
            ai_chat_cancel,
            ai_tool_resolve,
            chat_list_sessions,
            chat_create_session,
            chat_get_session,
            chat_rename_session,
            chat_delete_session,
            chat_clear_session,
            ai_keychain_get,
            ai_keychain_set,
            ai_keychain_delete,
            mcp_list_servers,
            mcp_add_server,
            mcp_remove_server,
            mcp_set_server_enabled,
            mcp_list_tools,
            mcp_connection_states,
            mcp_call_tool,
            mcp_server_status,
            mcp_server_start,
            mcp_server_stop,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // 退出 flush: 把未落盘的工作区变更同步写盘 (逻辑在 app_state::runtime)
            if let tauri::RunEvent::Exit = event {
                app_state::flush_workspace_on_exit(app);
            }
        });
}
