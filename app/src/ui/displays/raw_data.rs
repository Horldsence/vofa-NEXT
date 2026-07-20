//! 原始数据 Hex + ASCII 视图
//!
//! 每帧从 `state.raw_data_collector` 抽取 (`drain_batch`) 到 UI 侧滚动缓冲,
//! 以 16 字节/行渲染 offset + hex + ASCII。
//! Clear 按钮同时清空 UI 缓冲和后端收集器 (rt.spawn 异步调用)。

use std::collections::VecDeque;
use std::sync::Arc;

use eframe::egui;

use crate::core::{services, AppState};

/// UI 侧滚动缓冲容量 (字节)
const ROLLING_CAPACITY: usize = 64 * 1024;
/// 单帧最多从后端抽取的字节数
const DRAIN_PER_FRAME: usize = 256 * 1024;
/// 最多渲染的字节数 (超出部分只显示末尾, 避免长文本卡顿)
const MAX_RENDER_BYTES: usize = 32 * 1024;
/// 每行字节数
const BYTES_PER_ROW: usize = 16;

/// 原始数据页签的持久化状态
pub struct RawDataState {
    /// 滚动字节缓冲
    bytes: VecDeque<u8>,
    /// bytes[0] 对应的绝对偏移 (已丢弃字节数)
    base_offset: u64,
    /// 后端累计接收字节数
    total_bytes: u64,
    /// 后端累计丢弃字节数
    dropped_bytes: u64,
    /// 自动滚动到底部
    follow_tail: bool,
}

impl RawDataState {
    pub fn new() -> Self {
        Self {
            bytes: VecDeque::with_capacity(ROLLING_CAPACITY),
            base_offset: 0,
            total_bytes: 0,
            dropped_bytes: 0,
            follow_tail: true,
        }
    }

    fn clear(&mut self) {
        self.bytes.clear();
        self.base_offset = 0;
    }

    fn push_chunk(&mut self, chunk: &[u8]) {
        self.bytes.extend(chunk.iter().copied());
        while self.bytes.len() > ROLLING_CAPACITY {
            self.bytes.pop_front();
            self.base_offset += 1;
        }
    }
}

impl Default for RawDataState {
    fn default() -> Self {
        Self::new()
    }
}

/// 渲染原始数据 Data 页签
pub fn show(
    ui: &mut egui::Ui,
    state: &Arc<AppState>,
    rt: &tokio::runtime::Runtime,
    view: &mut RawDataState,
) {
    // 每帧从后端收集器抽取新数据
    let batch = state.raw_data_collector.lock().drain_batch(DRAIN_PER_FRAME);
    view.total_bytes = batch.total_bytes;
    view.dropped_bytes = batch.dropped_bytes;
    for chunk in &batch.chunks {
        view.push_chunk(&chunk.bytes);
    }

    ui.horizontal(|ui| {
        if ui.button("Clear").clicked() {
            view.clear();
            let state = state.clone();
            rt.spawn(async move {
                let _ = services::clear_raw_data_collector(&state).await;
            });
        }
        ui.checkbox(&mut view.follow_tail, "Follow tail");
        ui.separator();
        ui.label(format!("buffered: {} B", view.bytes.len()));
        ui.label(format!("total: {} B", view.total_bytes));
        if view.dropped_bytes > 0 {
            ui.label(format!("dropped: {} B", view.dropped_bytes));
        }
    });
    ui.separator();

    if view.bytes.is_empty() {
        ui.label("No raw data received yet.");
        return;
    }

    // 只渲染末尾 MAX_RENDER_BYTES, 起始位置对齐到行边界
    let render_len = view.bytes.len().min(MAX_RENDER_BYTES);
    let start = view.bytes.len() - render_len;
    let aligned_start = start - (start % BYTES_PER_ROW);

    let mut text = String::with_capacity(render_len * 4);
    let mut row_start = aligned_start;
    while row_start < view.bytes.len() {
        let row_end = (row_start + BYTES_PER_ROW).min(view.bytes.len());
        let offset = view.base_offset + row_start as u64;
        text.push_str(&format!("{offset:08X}  "));

        let mut ascii = String::with_capacity(BYTES_PER_ROW);
        for i in row_start..row_end {
            let b = view.bytes[i];
            text.push_str(&format!("{b:02X} "));
            ascii.push(if b.is_ascii_graphic() || b == b' ' {
                b as char
            } else {
                '.'
            });
        }
        // 补齐不满一行的 hex 区, 使 ASCII 列对齐
        for _ in row_end..row_start + BYTES_PER_ROW {
            text.push_str("   ");
        }
        text.push_str("  ");
        text.push_str(&ascii);
        text.push('\n');
        row_start = row_end;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(view.follow_tail)
        .show(ui, |ui| {
            ui.add(egui::Label::new(egui::RichText::new(text).monospace()).wrap_mode(egui::TextWrapMode::Extend));
        });

    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(100));
}
