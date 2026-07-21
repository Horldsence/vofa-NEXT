//! # window — 窗口视觉效果命令

use tauri::{Runtime, WebviewWindow};

/// 启用/关闭窗口亚克力（毛玻璃）背景效果。
///
/// 前端配合把背景 token 转为半透明后调用本命令。
/// `blur_radius` 仅 macOS 生效, 0 或 None 表示使用系统默认半径。
/// 已知限制: Windows 上 transparent 窗口会丢失原生阴影/圆角, 且 Acrylic
/// 拖动时可能有残影（系统限制）; Linux 不支持, 为 no-op。
#[tauri::command]
pub fn set_window_acrylic<R: Runtime>(
    window: WebviewWindow<R>,
    enabled: bool,
    blur_radius: Option<f64>,
) {
    // NSVisualEffectView / DWM 操作必须在主线程执行;
    // 与 tauri 内部 set_effects 一致, 失败仅记录日志, 不向前端报错。
    let _ = window.clone().run_on_main_thread(move || {
        #[cfg(target_os = "macos")]
        {
            use window_vibrancy::{
                NSVisualEffectMaterial, NSVisualEffectState, apply_vibrancy, clear_vibrancy,
            };
            let radius = blur_radius.filter(|r| *r > 0.0);
            let result = if enabled {
                apply_vibrancy(
                    &window,
                    NSVisualEffectMaterial::UnderWindowBackground,
                    Some(NSVisualEffectState::FollowsWindowActiveState),
                    radius,
                )
                .map(|_| ())
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
            let _ = blur_radius; // Windows Acrylic 不支持模糊半径参数
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
        let _ = (&window, enabled, blur_radius);
    });
}
