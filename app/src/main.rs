mod app;
mod core;
mod settings;
mod theme;
mod ui;

use std::sync::Arc;

use app::VofaApp;

fn main() -> eframe::Result {
    tracing_subscriber::fmt::init();

    // Tokio 多线程运行时 — UI 与 core::services 通过 Arc 共享,
    // 用于 spawn 异步 service 调用 (transport open/close, 数据回传等)。
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("vofa-tokio")
        .build()
        .expect("failed to build tokio runtime");
    let rt = Arc::new(rt);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "VOFA-NEXT",
        options,
        Box::new(move |_cc| Ok(Box::new(VofaApp::new(rt)))),
    )
}
