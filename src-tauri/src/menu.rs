//! 原生菜单栏 (macOS/Windows/Linux)
//!
//! 菜单项 ID 通过 `on_menu_event` 转发到前端:
//! - `menu:about`       -> 打开 About 弹窗
//! - `menu:settings`    -> 打开 Settings 弹窗
//! - `menu:new-tab`     -> 在控件区新建 Tab
//! - `menu:close-tab`   -> 关闭当前 Tab
//! - `menu:toggle-sidebar` -> 折叠/展开左侧栏
//! - `menu:reload`      -> 重新加载前端
//! - `menu:zoom-in`     -> 放大
//! - `menu:zoom-out`    -> 缩小
//! - `menu:zoom-reset`  -> 重置缩放
//! - `menu:github`      -> 打开 GitHub 仓库
//! - `menu:docs`        -> 打开文档

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    App, Emitter, Manager, Wry,
};

/// 菜单项 ID 常量 — 与前端事件名一致
pub mod ids {
    pub const ABOUT: &str = "menu:about";
    pub const SETTINGS: &str = "menu:settings";
    pub const NEW_TAB: &str = "menu:new-tab";
    pub const CLOSE_TAB: &str = "menu:close-tab";
    pub const TOGGLE_SIDEBAR: &str = "menu:toggle-sidebar";
    pub const RELOAD: &str = "menu:reload";
    pub const ZOOM_IN: &str = "menu:zoom-in";
    pub const ZOOM_OUT: &str = "menu:zoom-out";
    pub const ZOOM_RESET: &str = "menu:zoom-reset";
    pub const GITHUB: &str = "menu:github";
    pub const DOCS: &str = "menu:docs";
}

/// 构建应用主菜单
pub fn build_menu(app: &App) -> tauri::Result<Menu<Wry>> {
    let app_handle = app.handle();

    // ============ App 菜单 (macOS 首个菜单, 与应用名同名) ============
    let app_menu = Submenu::with_items(
        app_handle,
        "VOFA-Next",
        true,
        &[
            &MenuItem::with_id(app_handle, ids::ABOUT, "About VOFA-Next", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app_handle)?,
            &MenuItem::with_id(
                app_handle,
                ids::SETTINGS,
                "Settings...",
                true,
                Some("CmdOrCtrl+,"),
            )?,
            &PredefinedMenuItem::separator(app_handle)?,
            &PredefinedMenuItem::hide(app_handle, None)?,
            &PredefinedMenuItem::hide_others(app_handle, None)?,
            &PredefinedMenuItem::show_all(app_handle, None)?,
            &PredefinedMenuItem::separator(app_handle)?,
            &PredefinedMenuItem::quit(app_handle, Some("Quit VOFA-Next"))?,
        ],
    )?;

    // ============ File 菜单 ============
    let file_menu = Submenu::with_items(
        app_handle,
        "File",
        true,
        &[
            &MenuItem::with_id(app_handle, ids::NEW_TAB, "New Tab", true, Some("CmdOrCtrl+T"))?,
            &MenuItem::with_id(
                app_handle,
                ids::CLOSE_TAB,
                "Close Tab",
                true,
                Some("CmdOrCtrl+W"),
            )?,
            &PredefinedMenuItem::separator(app_handle)?,
        ],
    )?;

    // ============ Edit 菜单 (预定义项自动接管文本编辑) ============
    let edit_menu = Submenu::with_items(
        app_handle,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app_handle, None)?,
            &PredefinedMenuItem::redo(app_handle, None)?,
            &PredefinedMenuItem::separator(app_handle)?,
            &PredefinedMenuItem::cut(app_handle, None)?,
            &PredefinedMenuItem::copy(app_handle, None)?,
            &PredefinedMenuItem::paste(app_handle, None)?,
            &PredefinedMenuItem::select_all(app_handle, None)?,
        ],
    )?;

    // ============ View 菜单 ============
    let view_menu = Submenu::with_items(
        app_handle,
        "View",
        true,
        &[
            &MenuItem::with_id(
                app_handle,
                ids::TOGGLE_SIDEBAR,
                "Toggle Sidebar",
                true,
                Some("CmdOrCtrl+B"),
            )?,
            &PredefinedMenuItem::separator(app_handle)?,
            &MenuItem::with_id(app_handle, ids::RELOAD, "Reload", true, Some("CmdOrCtrl+R"))?,
            &MenuItem::with_id(app_handle, ids::ZOOM_IN, "Zoom In", true, Some("CmdOrCtrl+="))?,
            &MenuItem::with_id(app_handle, ids::ZOOM_OUT, "Zoom Out", true, Some("CmdOrCtrl+-"))?,
            &MenuItem::with_id(
                app_handle,
                ids::ZOOM_RESET,
                "Actual Size",
                true,
                Some("CmdOrCtrl+0"),
            )?,
        ],
    )?;

    // ============ Window 菜单 (预定义最小化/缩放) ============
    let window_menu = Submenu::with_items(
        app_handle,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app_handle, None)?,
            &PredefinedMenuItem::maximize(app_handle, None)?,
            &PredefinedMenuItem::close_window(app_handle, None)?,
        ],
    )?;

    // ============ Help 菜单 ============
    let help_menu = Submenu::with_items(
        app_handle,
        "Help",
        true,
        &[
            &MenuItem::with_id(app_handle, ids::DOCS, "Documentation", true, None::<&str>)?,
            &MenuItem::with_id(app_handle, ids::GITHUB, "Open on GitHub", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app_handle)?,
            &MenuItem::with_id(app_handle, ids::ABOUT, "About VOFA-Next", true, None::<&str>)?,
        ],
    )?;

    Menu::with_items(
        app_handle,
        &[&app_menu, &file_menu, &edit_menu, &view_menu, &window_menu, &help_menu],
    )
}

/// 处理菜单点击事件 — 转发到前端或执行内置操作
pub fn on_menu_event(app: &tauri::AppHandle, id: &str) {
    match id {
        ids::GITHUB => {
            if let Err(e) = tauri_plugin_opener::open_url(
                "https://github.com/pengheng/vofa-next",
                None::<&str>,
            ) {
                log::warn!("打开 GitHub 失败: {}", e);
            }
        }
        ids::DOCS => {
            if let Err(e) = tauri_plugin_opener::open_url(
                "https://github.com/pengheng/vofa-next#readme",
                None::<&str>,
            ) {
                log::warn!("打开文档失败: {}", e);
            }
        }
        ids::RELOAD => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.eval("window.location.reload()");
            }
        }
        ids::ZOOM_IN => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.eval("window.zoomLevel = Math.min((window.zoomLevel||0)+0.2, 3);");
            }
        }
        ids::ZOOM_OUT => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.eval("window.zoomLevel = Math.max((window.zoomLevel||0)-0.2, -2);");
            }
        }
        ids::ZOOM_RESET => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.eval("window.zoomLevel = 0;");
            }
        }
        // 其它菜单项统一转发到前端, 由前端处理
        _ => {
            if let Err(e) = app.emit("menu-event", id) {
                log::warn!("转发菜单事件失败: {}", e);
            }
        }
    }
}
