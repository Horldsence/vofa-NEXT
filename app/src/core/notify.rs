//! 通知辅助模块 — 基于 notify-rust 的系统通知
//!
//! 设计目标:
//! - 仅在用户关心的事件触发系统通知 (避免噪声)
//! - 失败安全: 通知本身失败不影响业务逻辑 (仅记录日志)
//! - 统一 title 前缀 "VOFA-Next"

use vofa_next_core::TransportConfig;

const APP_TITLE: &str = "VOFA-Next";

/// 推送系统通知 (内部实现, 失败安全)
fn show(title: &str, body: &str) {
    if let Err(e) = notify_rust::Notification::new()
        .summary(title)
        .body(body)
        .show()
    {
        tracing::warn!("系统通知发送失败: {}", e);
    }
}

/// 推送普通通知
pub fn info(body: impl AsRef<str>) {
    show(APP_TITLE, body.as_ref());
}

/// 推送错误通知 (title 标注错误)
pub fn error(body: impl AsRef<str>) {
    show(&format!("{} 错误", APP_TITLE), body.as_ref());
}

/// 连接已建立 — 由 open_transport 成功路径调用
pub fn connected(kind: &str) {
    info(format!("已连接: {}", kind));
}

/// 连接已断开 — 由 close_transport 或异常退出路径调用
pub fn disconnected() {
    info("连接已断开");
}

/// 自动通道检测完成
pub fn channels_detected(count: usize) {
    info(format!("检测到 {} 个通道", count));
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
