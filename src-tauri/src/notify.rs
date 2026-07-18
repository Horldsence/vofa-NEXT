//! 通知辅助模块 — 封装 tauri-plugin-notification
//!
//! 设计目标:
//! - 仅在用户关心的事件触发系统通知 (避免噪声)
//! - 失败安全: 通知本身失败不影响业务逻辑
//! - 统一 title 前缀 "VOFA-Next"

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;
use vofa_next_core::TransportConfig;

const APP_TITLE: &str = "VOFA-Next";

/// 推送普通通知
pub fn info(app: &AppHandle, body: impl AsRef<str>) {
    let _ = app
        .notification()
        .builder()
        .title(APP_TITLE)
        .body(body.as_ref())
        .show();
}

/// 推送错误通知 (title 标注错误)
pub fn error(app: &AppHandle, body: impl AsRef<str>) {
    let _ = app
        .notification()
        .builder()
        .title(format!("{} 错误", APP_TITLE))
        .body(body.as_ref())
        .show();
}

/// 连接已建立 — 由 open_transport 成功路径调用
pub fn connected(app: &AppHandle, kind: &str) {
    info(app, format!("已连接: {}", kind));
}

/// 连接已断开 — 由 close_transport 或异常退出路径调用
pub fn disconnected(app: &AppHandle) {
    info(app, "连接已断开");
}

/// 自动通道检测完成
pub fn channels_detected(app: &AppHandle, count: usize) {
    info(app, format!("检测到 {} 个通道", count));
}

/// 从 TransportConfig 提取简洁字符串 (用于通知)
pub fn transport_kind_str(config: &TransportConfig) -> &'static str {
    match config {
        TransportConfig::Serial(_) => "Serial",
        TransportConfig::Udp(_) => "UDP",
        TransportConfig::TcpClient(_) => "TCP Client",
        TransportConfig::TcpServer(_) => "TCP Server",
        TransportConfig::TestData(_) => "Test Data",
        TransportConfig::Slcan(_) => "slcan",
        TransportConfig::CandleLight(_) => "candleLight",
    }
}
