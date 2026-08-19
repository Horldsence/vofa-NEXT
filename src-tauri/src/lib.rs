mod commands;
mod events;
mod menu;
mod notify;
mod pipeline;
mod state;
mod subscription;
mod update;

use state::AppState;
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
        .manage(update::PendingUpdate(Default::default()))
        .setup(|app| {
            // 构建原生菜单栏。
            // - macOS: 菜单位于系统全局菜单栏, 与窗口透明无关, 正常挂载。
            // - Linux: GTK 菜单栏自带不透明背景, 无此问题, 正常挂载。
            // - Windows: 主窗口为透明窗口 (WS_EX_LAYERED) 时, 原生菜单栏无法正确绘制:
            //   菜单文字按深色模式渲染为白色, 但菜单背景在分层窗口上不填充, 造成
            //   "白字白底" 且背景透视露出下层内容。因此 Windows 仅构建但不挂载原生菜单,
            //   改由前端自定义菜单栏 (MenuBar.tsx) 承担同等功能。
            let menu = menu::build_menu(app)?;
            #[cfg(not(target_os = "windows"))]
            app.set_menu(menu)?;

            // 启动图输出 ticker (60 FPS 推送快照到前端)
            let eval_state_for_ticker = {
                let state = app.state::<AppState>();
                state.eval_state()
            };
            tauri::async_runtime::spawn(state::graph_output_ticker(eval_state_for_ticker));

            // 启动 Custom 输入 ticker (30 FPS 推送到 iframe)
            let eval_state_for_custom = {
                let state = app.state::<AppState>();
                state.eval_state()
            };
            tauri::async_runtime::spawn(state::custom_input_ticker(eval_state_for_custom));

            // 启动频谱分析 ticker (30 FFT 计算 + 推送 SpectrumBatch)
            let eval_state_for_spectrum = {
                let state = app.state::<AppState>();
                state.eval_state()
            };
            tauri::async_runtime::spawn(state::spectrum_ticker(eval_state_for_spectrum));

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
        .on_menu_event(|app, event| menu::on_menu_event(app, event.id().as_ref()))
        .invoke_handler(tauri::generate_handler![
            // 传输
            commands::list_ports,
            commands::open_transport,
            commands::close_transport,
            commands::send_raw,
            commands::send_string,
            commands::send_widget_value,
            commands::send_and_capture,
            commands::get_connection_state,
            commands::get_stats,
            commands::start_test_data,
            commands::stop_test_data,
            commands::get_test_data_state,
            commands::update_transport_protocol,
            // 协议
            commands::set_protocol,
            commands::get_protocol,
            commands::get_detected_channels,
            // 流水线参数
            commands::set_pipeline_config,
            commands::get_pipeline_config,
            // 波形缓冲区
            commands::subscribe_waveform,
            commands::get_recent_waveform,
            commands::get_waveform_window,
            commands::clear_buffer,
            commands::set_buffer_channels,
            commands::get_buffer_info,
            commands::set_waveform_buffer_capacity,
            commands::set_rawdata_buffer_capacity,
            commands::set_can_buffer_capacity,
            commands::set_logic_buffer_capacity,
            // 节点图 (后端化重构)
            commands::update_tab_graph,
            commands::remove_tab_graph,
            commands::set_input_value,
            commands::submit_custom_output,
            commands::inject_bytes,
            commands::subscribe_graph_outputs,
            commands::subscribe_custom_inputs,
            commands::subscribe_spectrum,
            commands::unsubscribe_graph_outputs,
            commands::unsubscribe_custom_inputs,
            commands::unsubscribe_spectrum,
            commands::unsubscribe_waveform,
            // 原始数据
            commands::subscribe_rawdata,
            commands::unsubscribe_rawdata,
            commands::subscribe_rawdata_node,
            commands::unsubscribe_rawdata_node,
            commands::subscribe_rawdata_filtered,
            commands::subscribe_rawdata_node_filtered,
            commands::clear_raw_data_collector,
            // CAN 帧
            commands::send_can_frame,
            commands::subscribe_can_frames,
            commands::subscribe_can_frames_filtered,
            commands::unsubscribe_can_frames,
            commands::get_recent_can_frames,
            commands::clear_can_buffer,
            commands::get_can_buffer_info,
            commands::list_candle_devices,
            // 逻辑分析仪
            commands::subscribe_logic_samples,
            commands::subscribe_logic_samples_filtered,
            commands::unsubscribe_logic_samples,
            commands::get_recent_logic_samples,
            commands::clear_logic_buffer,
            commands::get_logic_buffer_info,
            commands::subscribe_decoded_events,
            commands::subscribe_decoded_events_filtered,
            commands::unsubscribe_decoded_events,
            commands::get_recent_decoded_events,
            commands::clear_decoded_buffer,
            commands::get_decoded_buffer_info,
            // CAN 负载分析
            commands::get_can_load_stats,
            commands::set_can_load_window,
            commands::clear_can_load_stats,
            commands::subscribe_can_load,
            commands::unsubscribe_can_load,
            commands::get_current_can_bitrate,
            commands::export_can_load_csv,
            // 帧解码器手动测试 (FrameDecoder 面板)
            commands::parse_frame_decoder_input,
            // 调试
            commands::inspect_element,
            // 窗口
            commands::set_window_acrylic,
            commands::close_splashscreen,
            // 应用更新
            update::check_update,
            update::download_and_install_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
