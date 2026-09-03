//! # window — 窗口视觉效果命令

use tauri::{Runtime, WebviewWindow};

/// 启用/关闭窗口亚克力（毛玻璃）背景效果。
///
/// 前端配合把背景 token 转为半透明后调用本命令。
/// 已知限制: NSVisualEffectView 不支持自定义模糊半径; Windows 上 transparent
/// 窗口会丢失原生阴影/圆角, 且 Acrylic 拖动时可能有残影（系统限制）;
/// Linux 不支持, 为 no-op。
#[tauri::command]
pub fn set_window_acrylic<R: Runtime>(window: WebviewWindow<R>, enabled: bool) {
    // NSVisualEffectView / DWM 操作必须在主线程执行;
    // 与 tauri 内部 set_effects 一致, 失败仅记录日志, 不向前端报错。
    let _ = window.clone().run_on_main_thread(move || {
        #[cfg(target_os = "macos")]
        {
            use window_vibrancy::{
                apply_vibrancy, clear_vibrancy, NSVisualEffectMaterial, NSVisualEffectState,
            };
            // FullScreenUI 跟随系统深浅外观, 能真实透出窗口后方内容;
            // UnderWindowBackground 只采样桌面壁纸且接近不透明, 会让亚克力看起来失效。
            let result = if enabled {
                apply_vibrancy(
                    &window,
                    NSVisualEffectMaterial::FullScreenUI,
                    Some(NSVisualEffectState::FollowsWindowActiveState),
                    None,
                )
            } else {
                clear_vibrancy(&window).map(|_| ())
            };
            if let Err(e) = result {
                log::warn!("set_window_acrylic failed: {e}");
            }
        }
        #[cfg(target_os = "windows")]
        {
            use window_vibrancy::{apply_acrylic, clear_acrylic};
            let result = if enabled {
                apply_acrylic(&window, None::<(u8, u8, u8, u8)>)
            } else {
                clear_acrylic(&window)
            };
            if let Err(e) = result {
                log::warn!("set_window_acrylic failed: {e}");
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let _ = (&window, enabled);
    });
}

/// 关闭启动页窗口并显示主窗口。
///
/// 前端在应用初始化完成（设置加载完毕、首帧已渲染）后调用本命令。
/// 两个窗口都可能已不存在（如重复调用），失败仅记录日志。
#[tauri::command]
pub fn close_splashscreen(app: tauri::AppHandle) {
    use tauri::Manager;
    if let Some(splash) = app.get_webview_window("splashscreen") {
        if let Err(e) = splash.close() {
            log::warn!("close splashscreen failed: {e}");
        }
    }
    if let Some(main) = app.get_webview_window("main") {
        if let Err(e) = main.show().and_then(|()| main.set_focus()) {
            log::warn!("show main window failed: {e}");
        }
    }
}
