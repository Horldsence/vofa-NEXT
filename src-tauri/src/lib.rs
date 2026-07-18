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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
