//! # debug — 调试相关命令

use tauri::{Runtime, WebviewWindow};

/// 打开当前 Webview 的开发者工具（检查元素）。
///
/// 在 release 构建中需要启用 `devtools` Cargo feature 才能正常工作。
#[tauri::command]
pub fn inspect_element<R: Runtime>(window: WebviewWindow<R>) {
    window.open_devtools();
}
