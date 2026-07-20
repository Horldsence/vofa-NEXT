//! 逻辑分析仪显示 — 数字时序图 + 解码事件表
//!
//! 时序图用 `egui::Painter` 手绘: 每通道一条水平轨道, 高低电平画阶梯折线。
//! 下方为最近 200 条解码事件 (UART / I2C / SPI) 表格。
//! `Decoded` Data 页签直接复用 [`show_decoded_events`]。

use std::sync::Arc;

use eframe::egui;
use vofa_next_core::{DecodedEvent, I2cEvent, LogicSample};

use crate::core::AppState;

/// 时序图显示的采样数上限
const MAX_SAMPLES: usize = 500;
/// 解码事件表显示条数上限
const MAX_EVENTS: usize = 200;
/// 最多渲染的通道数
const MAX_CHANNELS: usize = 8;

/// 渲染逻辑分析仪 Data 页签
pub fn show(ui: &mut egui::Ui, state: &Arc<AppState>) {
    let samples = state.logic_buffer.lock().get_recent(MAX_SAMPLES);

    ui.label(format!("{} samples", samples.len()));
    ui.separator();

    if samples.is_empty() {
        ui.label("No logic samples received yet.");
    } else {
        show_timing_diagram(ui, &samples);
    }

    ui.add_space(8.0);
    ui.separator();
    ui.strong("Decoded events");
    show_decoded_events(ui, state);

    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(100));
}

/// 数字时序图 (阶梯波形)
fn show_timing_diagram(ui: &mut egui::Ui, samples: &[LogicSample]) {
    let ch_count = samples
        .iter()
        .map(|s| s.channel_count as usize)
        .max()
        .unwrap_or(0)
        .clamp(1, MAX_CHANNELS);

    let label_w = 32.0;
    let track_h = 26.0;
    let high_h = 8.0; // 高电平距轨道顶部的偏移
    let low_h = track_h - 6.0; // 低电平距轨道顶部的偏移

    let width = ui.available_width();
    let height = track_h * ch_count as f32 + 4.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter_at(rect);

    let plot_left = rect.left() + label_w;
    let plot_right = rect.right();
    let plot_w = (plot_right - plot_left).max(1.0);

    let t0 = samples.first().map(|s| s.timestamp).unwrap_or(0);
    let t1 = samples.last().map(|s| s.timestamp).unwrap_or(0);
    let span = (t1.saturating_sub(t0)).max(1) as f64;
    let x_of = |t: u64| -> f32 { plot_left + ((t - t0) as f64 / span) as f32 * plot_w };

    let frame_stroke = egui::Stroke::new(1.0, ui.visuals().weak_text_color());
    let wave_stroke = egui::Stroke::new(1.5, egui::Color32::from_rgb(0x4C, 0xAF, 0x50));

    for ch in 0..ch_count {
        let track_top = rect.top() + ch as f32 * track_h;
        let y_high = track_top + high_h;
        let y_low = track_top + low_h;
        let y_of = |level: bool| if level { y_high } else { y_low };

        // 通道标签与轨道分隔线
        painter.text(
            egui::pos2(rect.left() + 2.0, track_top + track_h / 2.0),
            egui::Align2::LEFT_CENTER,
            format!("D{ch}"),
            egui::FontId::monospace(11.0),
            ui.visuals().text_color(),
        );
        painter.line_segment(
            [
                egui::pos2(plot_left, track_top + track_h),
                egui::pos2(plot_right, track_top + track_h),
            ],
            frame_stroke,
        );

        // 阶梯折线: 每个采样从其时刻水平延伸到下一采样, 电平变化处画竖线
        let mut points = Vec::with_capacity(samples.len() * 2);
        let mut prev_level: Option<bool> = None;
        for (i, s) in samples.iter().enumerate() {
            let level = (s.channels >> ch) & 1 == 1;
            let x = x_of(s.timestamp);
            if let Some(prev) = prev_level {
                if prev != level {
                    // 先水平到跳变点, 再竖直跳变
                    points.push(egui::pos2(x, y_of(prev)));
                    points.push(egui::pos2(x, y_of(prev)));
                    points.push(egui::pos2(x, y_of(level)));
                } else {
                    points.push(egui::pos2(x, y_of(level)));
                }
            } else {
                points.push(egui::pos2(x, y_of(level)));
            }
            prev_level = Some(level);
            // 最后一个采样延伸到右边界
            if i == samples.len() - 1 {
                points.push(egui::pos2(plot_right, y_of(level)));
            }
        }
        if points.len() >= 2 {
            painter.add(egui::Shape::line(points, wave_stroke));
        }
    }
}

/// 解码事件表格 (Logic 与 Decoded 页签共用)
pub fn show_decoded_events(ui: &mut egui::Ui, state: &Arc<AppState>) {
    let events = state.decoded_buffer.lock().get_recent(MAX_EVENTS);

    if events.is_empty() {
        ui.label("No decoded events yet.");
        return;
    }

    egui::Grid::new("decoded_events_grid")
        .striped(true)
        .num_columns(3)
        .show(ui, |ui| {
            ui.strong("Time (µs)");
            ui.strong("Proto");
            ui.strong("Detail");
            ui.end_row();
            // 最新在前
            for event in events.iter().rev() {
                match event {
                    DecodedEvent::Uart {
                        timestamp,
                        byte,
                        parity_ok,
                    } => {
                        ui.monospace(timestamp.to_string());
                        ui.monospace("UART");
                        let ch = if byte.is_ascii_graphic() || *byte == b' ' {
                            format!(" '{}'", *byte as char)
                        } else {
                            String::new()
                        };
                        let parity = if *parity_ok { "" } else { " (parity err)" };
                        ui.monospace(format!("0x{byte:02X}{ch}{parity}"));
                    }
                    DecodedEvent::I2c { timestamp, event } => {
                        ui.monospace(timestamp.to_string());
                        ui.monospace("I2C");
                        ui.monospace(format_i2c_event(event));
                    }
                    DecodedEvent::Spi {
                        timestamp,
                        mosi,
                        miso,
                    } => {
                        ui.monospace(timestamp.to_string());
                        ui.monospace("SPI");
                        ui.monospace(format!("MOSI 0x{mosi:02X}  MISO 0x{miso:02X}"));
                    }
                }
                ui.end_row();
            }
        });
}

fn format_i2c_event(event: &I2cEvent) -> String {
    match event {
        I2cEvent::Start => "START".to_string(),
        I2cEvent::Stop => "STOP".to_string(),
        I2cEvent::Address { addr, read, ack } => {
            let rw = if *read { "R" } else { "W" };
            let ack = if *ack { "ACK" } else { "NACK" };
            format!("ADDR 0x{addr:02X} {rw} {ack}")
        }
        I2cEvent::Data { byte, ack } => {
            let ack = if *ack { "ACK" } else { "NACK" };
            format!("DATA 0x{byte:02X} {ack}")
        }
    }
}
