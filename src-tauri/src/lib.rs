mod commands;
mod notify;
mod state;

use state::AppState;
use tauri_plugin_log::{Target, TargetKind};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
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
        .manage(AppState::new())
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
            // 节点图
            commands::update_node_graph,
            commands::get_node_edges,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
