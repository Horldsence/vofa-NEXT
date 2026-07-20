//! # ui — egui 界面层
//!
//! - [`layout`][]: 应用外壳布局 (菜单栏 / 状态栏 / 活动栏 / 侧栏)
//! - [`dock`][]: 中央停靠区 (egui_dock) 的 Tab 定义与渲染

pub mod controls;
pub mod displays;
pub mod dock;
pub mod layout;
pub mod node_editor;
pub mod panels;
pub mod toasts;
