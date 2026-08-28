// 直接引用 cmd_* crates 中的 Tauri 命令
pub use cmd_buffer::*;
pub use cmd_can_load::*;
pub use cmd_can_transport::*;
pub use cmd_debug::*;
pub use cmd_graph::*;
pub use cmd_pipeline::*;
pub use cmd_rawdata::*;

pub use app_state::{
    custom_input_ticker, graph_output_ticker, spectrum_ticker, text_output_ticker,
    textout_sender_ticker, AppState,
};
pub use menu_shell::ids;
pub use menu_shell::{build_menu, on_menu_event};
// 必须 glob 导入: `#[tauri::command]` 生成的 `__cmd__*` / `__tauri_command_name_*`
// 宏 (`#[macro_export]`, doc-hidden) 只在 glob 导入时进入本 crate 宏命名空间,
// `generate_handler!` 展开时需要它们在作用域内。
use tauri::Manager;
use tauri_plugin_log::{Target, TargetKind};
pub use update_flow::*;

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

            // 启动图输出 ticker (60 FPS 推送快照到前端)
            let eval_state_for_ticker = {
                let state = app.state::<AppState>();
                state.eval_state()
            };
            tauri::async_runtime::spawn(graph_output_ticker(eval_state_for_ticker));

            // 启动 Custom 输入 ticker (30 FPS 推送到 iframe)
            let eval_state_for_custom = {
                let state = app.state::<AppState>();
                state.eval_state()
            };
            tauri::async_runtime::spawn(custom_input_ticker(eval_state_for_custom));

            // 启动字符串输出 ticker (30 FPS 推送给 TextDisplay)
            let eval_state_for_text = {
                let state = app.state::<AppState>();
                state.eval_state()
            };
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
            let eval_state_for_spectrum = {
                let state = app.state::<AppState>();
                state.eval_state()
            };
            tauri::async_runtime::spawn(spectrum_ticker(eval_state_for_spectrum));

            // 启动页兜底: 前端应在初始化完成后调用 close_splashscreen 关闭启动页;
            // 若前端异常迟迟未调用, 超时强制切换, 防止永远卡在启动页
            let fallback_handle = app.handle().clone();
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
            subscribe_waveform,
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
            set_input_value,
            submit_custom_output,
            submit_custom_text_output,
            inject_bytes,
            subscribe_graph_outputs,
            subscribe_custom_inputs,
            subscribe_string_outputs,
            subscribe_spectrum,
            unsubscribe_graph_outputs,
            unsubscribe_custom_inputs,
            unsubscribe_string_outputs,
            unsubscribe_spectrum,
            unsubscribe_waveform,
            // 原始数据
            subscribe_rawdata,
            unsubscribe_rawdata,
            subscribe_rawdata_node,
            unsubscribe_rawdata_node,
            subscribe_rawdata_filtered,
            subscribe_rawdata_node_filtered,
            clear_raw_data_collector,
            // CAN 帧
            send_can_frame,
            subscribe_can_frames,
            subscribe_can_frames_filtered,
            unsubscribe_can_frames,
            get_recent_can_frames,
            clear_can_buffer,
            get_can_buffer_info,
            list_candle_devices,
            // 逻辑分析仪
            subscribe_logic_samples,
            subscribe_logic_samples_filtered,
            unsubscribe_logic_samples,
            get_recent_logic_samples,
            clear_logic_buffer,
            get_logic_buffer_info,
            subscribe_decoded_events,
            subscribe_decoded_events_filtered,
            unsubscribe_decoded_events,
            get_recent_decoded_events,
            clear_decoded_buffer,
            get_decoded_buffer_info,
            // CAN 负载分析
            get_can_load_stats,
            set_can_load_window,
            clear_can_load_stats,
            subscribe_can_load,
            unsubscribe_can_load,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
