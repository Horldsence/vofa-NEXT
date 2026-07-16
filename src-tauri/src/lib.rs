mod commands;
mod state;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,serial=debug".into()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::list_ports,
            commands::open_transport,
            commands::close_transport,
            commands::send_raw,
            commands::send_string,
            commands::send_widget_value,
            commands::get_connection_state,
            commands::get_stats,
            commands::set_protocol,
            commands::get_protocol,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
