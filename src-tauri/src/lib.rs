mod commands;
mod menu;
mod notify;
mod state;

use state::AppState;
use tauri::Manager;
use tauri_plugin_log::{Target, TargetKind};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: None }),
                    Target::new(TargetKind::Webview),
                ])
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(AppState::new())
        .setup(|app| {
            // 构建并设置原生菜单栏 (macOS/Windows/Linux)
            let menu = menu::build_menu(app)?;
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
            commands::get_connection_state,
            commands::get_stats,
            commands::start_test_data,
            commands::stop_test_data,
            commands::get_test_data_state,
            // 协议
            commands::set_protocol,
            commands::get_protocol,
            commands::get_detected_channels,
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
            commands::clear_raw_data_collector,
            // CAN 帧
            commands::send_can_frame,
            commands::subscribe_can_frames,
            commands::unsubscribe_can_frames,
            commands::get_recent_can_frames,
            commands::clear_can_buffer,
            commands::get_can_buffer_info,
            commands::list_candle_devices,
            // 逻辑分析仪
            commands::subscribe_logic_samples,
            commands::unsubscribe_logic_samples,
            commands::get_recent_logic_samples,
            commands::clear_logic_buffer,
            commands::get_logic_buffer_info,
            commands::subscribe_decoded_events,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
