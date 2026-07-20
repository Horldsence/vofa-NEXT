//! # core — 从 Tauri 后端迁移而来的应用核心
//!
//! 纯 Rust 实现, 不依赖 Tauri。UI (egui) 每帧直接读取 AppState 中的共享状态,
//! 通过 services 中的同步/异步函数驱动后端。
//!
//! 迁移期允许 dead_code / 未使用的重导出: services 中的函数将在后续 Phase 被 UI 接线使用。
#![allow(dead_code, unused_imports)]

pub mod notify;
pub mod pipeline;
pub mod services;
pub mod state;

pub use state::AppState;
