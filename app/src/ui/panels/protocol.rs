//! 协议配置面板 — JustFloat / FireWater / RawData / CAN / 逻辑解码

use std::sync::Arc;

use eframe::egui;
use parking_lot::Mutex;
use vofa_next_core::{LogicDecoderConfig, Parity, ProtocolConfig, StopBits};

use crate::core::{services, AppState};

/// 协议类型 (用于 ComboBox 选择)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProtocolKind {
    JustFloat,
    FireWater,
    RawData,
    Slcan,
    CandleLight,
    LogicDecode,
}

impl ProtocolKind {
    const ALL: [Self; 6] = [
        Self::JustFloat,
        Self::FireWater,
        Self::RawData,
        Self::Slcan,
        Self::CandleLight,
        Self::LogicDecode,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::JustFloat => "JustFloat",
            Self::FireWater => "FireWater",
            Self::RawData => "RawData",
            Self::Slcan => "slcan",
            Self::CandleLight => "candleLight",
            Self::LogicDecode => "Logic Decode",
        }
    }

    fn of(config: &ProtocolConfig) -> Option<Self> {
        match config {
            ProtocolConfig::JustFloat { .. } => Some(Self::JustFloat),
            ProtocolConfig::FireWater { .. } => Some(Self::FireWater),
            ProtocolConfig::RawData => Some(Self::RawData),
            ProtocolConfig::Slcan => Some(Self::Slcan),
            ProtocolConfig::CandleLight => Some(Self::CandleLight),
            ProtocolConfig::LogicDecode { .. } => Some(Self::LogicDecode),
            // Diagnostic 走独立管线, 面板不提供编辑
            ProtocolConfig::Diagnostic { .. } => None,
        }
    }
}

/// 逻辑解码器类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecoderKind {
    Uart,
    I2c,
    Spi,
}

impl DecoderKind {
    fn of(config: &LogicDecoderConfig) -> Self {
        match config {
            LogicDecoderConfig::Uart { .. } => Self::Uart,
            LogicDecoderConfig::I2c { .. } => Self::I2c,
            LogicDecoderConfig::Spi { .. } => Self::Spi,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Uart => "UART",
            Self::I2c => "I2C",
            Self::Spi => "SPI",
        }
    }
}

fn default_decoder() -> LogicDecoderConfig {
    LogicDecoderConfig::Uart {
        baud_rate: 115200,
        data_bits: 8,
        parity: Parity::None,
        stop_bits: StopBits::One,
        channel: 0,
    }
}

pub struct ProtocolPanel {
    /// 本地可编辑的协议配置 (Apply 时克隆提交)
    config: ProtocolConfig,
    /// 最近一次 Apply 的结果提示 (错误信息)
    status: Arc<Mutex<Option<String>>>,
}

impl Default for ProtocolPanel {
    fn default() -> Self {
        Self {
            config: ProtocolConfig::default(),
            status: Arc::new(Mutex::new(None)),
        }
    }
}

impl ProtocolPanel {
    pub fn ui(&mut self, ui: &mut egui::Ui, state: &Arc<AppState>, rt: &tokio::runtime::Runtime) {
        // 协议切换可能自动断开 TestData 连接, 定时重绘保持显示新鲜
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(500));

        let mut kind = ProtocolKind::of(&self.config).unwrap_or(ProtocolKind::JustFloat);
        egui::ComboBox::from_label("Protocol")
            .selected_text(kind.label())
            .show_ui(ui, |ui| {
                for k in ProtocolKind::ALL {
                    ui.selectable_value(&mut kind, k, k.label());
                }
            });
        if Some(kind) != ProtocolKind::of(&self.config) {
            self.config = match kind {
                ProtocolKind::JustFloat => ProtocolConfig::JustFloat { channels: Some(4) },
                ProtocolKind::FireWater => ProtocolConfig::FireWater { channels: Some(4) },
                ProtocolKind::RawData => ProtocolConfig::RawData,
                ProtocolKind::Slcan => ProtocolConfig::Slcan,
                ProtocolKind::CandleLight => ProtocolConfig::CandleLight,
                ProtocolKind::LogicDecode => ProtocolConfig::LogicDecode {
                    decoder: default_decoder(),
                },
            };
        }
        ui.add_space(4.0);

        match &mut self.config {
            ProtocolConfig::JustFloat { channels } | ProtocolConfig::FireWater { channels } => {
                channels_row(ui, channels);
            }
            ProtocolConfig::LogicDecode { decoder } => {
                decoder_form(ui, decoder);
            }
            _ => {
                ui.small("No configurable parameters.");
            }
        }

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);

        if ui.button("Apply").clicked() {
            let state = state.clone();
            let config = self.config.clone();
            let status = self.status.clone();
            rt.spawn(async move {
                let result = services::set_protocol(&state, config).await;
                *status.lock() = result.err().map(|e| e.to_string());
            });
        }

        // 当前生效配置 + 自动检测到的通道数
        ui.add_space(4.0);
        let applied = state.protocol_config.lock().clone();
        ui.small(format!(
            "Active: {}",
            ProtocolKind::of(&applied).map_or("Diagnostic", |k| k.label())
        ));
        let detected = state.protocol.lock().detected_channels();
        match detected {
            Some(n) => ui.small(format!("Detected channels: {n}")),
            None => ui.small("Detected channels: — (manual or none)"),
        };

        if let Some(msg) = self.status.lock().as_ref() {
            ui.add_space(4.0);
            ui.colored_label(egui::Color32::LIGHT_RED, msg);
        }
    }
}

/// channels 编辑: None = 自动检测, Some(n) = 手动指定
fn channels_row(ui: &mut egui::Ui, channels: &mut Option<usize>) {
    let mut auto = channels.is_none();
    if ui.checkbox(&mut auto, "Auto-detect channels").changed() {
        *channels = if auto { None } else { Some(4) };
    }
    if !auto {
        ui.horizontal(|ui| {
            ui.label("Channels:");
            ui.add(egui::DragValue::new(channels.get_or_insert(4)).range(1..=64));
        });
    }
}

/// 逻辑解码器配置 (有限编辑)
fn decoder_form(ui: &mut egui::Ui, decoder: &mut LogicDecoderConfig) {
    let mut kind = DecoderKind::of(decoder);
    egui::ComboBox::from_label("Decoder")
        .selected_text(kind.label())
        .show_ui(ui, |ui| {
            for k in [DecoderKind::Uart, DecoderKind::I2c, DecoderKind::Spi] {
                ui.selectable_value(&mut kind, k, k.label());
            }
        });
    if kind != DecoderKind::of(decoder) {
        *decoder = match kind {
            DecoderKind::Uart => default_decoder(),
            DecoderKind::I2c => LogicDecoderConfig::I2c {
                sda_channel: 0,
                scl_channel: 1,
            },
            DecoderKind::Spi => LogicDecoderConfig::Spi {
                sclk_channel: 0,
                mosi_channel: 1,
                miso_channel: 2,
                cs_channel: 3,
                mode: 0,
            },
        };
    }

    match decoder {
        LogicDecoderConfig::Uart {
            baud_rate,
            data_bits,
            parity,
            stop_bits,
            channel,
        } => {
            egui::ComboBox::from_label("Baud rate")
                .selected_text(baud_rate.to_string())
                .show_ui(ui, |ui| {
                    for b in [9600u32, 19200, 38400, 57600, 115200, 230400, 460800, 921600] {
                        ui.selectable_value(baud_rate, b, b.to_string());
                    }
                });
            egui::ComboBox::from_label("Data bits")
                .selected_text(data_bits.to_string())
                .show_ui(ui, |ui| {
                    for b in [5u8, 6, 7, 8] {
                        ui.selectable_value(data_bits, b, b.to_string());
                    }
                });
            egui::ComboBox::from_label("Parity")
                .selected_text(format!("{parity:?}"))
                .show_ui(ui, |ui| {
                    for p in [Parity::None, Parity::Odd, Parity::Even] {
                        ui.selectable_value(parity, p, format!("{p:?}"));
                    }
                });
            egui::ComboBox::from_label("Stop bits")
                .selected_text(format!("{stop_bits:?}"))
                .show_ui(ui, |ui| {
                    for s in [StopBits::One, StopBits::Two] {
                        ui.selectable_value(stop_bits, s, format!("{s:?}"));
                    }
                });
            ui.horizontal(|ui| {
                ui.label("Channel:");
                ui.add(egui::DragValue::new(channel).range(0..=7));
            });
        }
        LogicDecoderConfig::I2c {
            sda_channel,
            scl_channel,
        } => {
            ui.horizontal(|ui| {
                ui.label("SDA channel:");
                ui.add(egui::DragValue::new(sda_channel).range(0..=7));
            });
            ui.horizontal(|ui| {
                ui.label("SCL channel:");
                ui.add(egui::DragValue::new(scl_channel).range(0..=7));
            });
        }
        LogicDecoderConfig::Spi {
            sclk_channel,
            mosi_channel,
            miso_channel,
            cs_channel,
            mode,
        } => {
            ui.horizontal(|ui| {
                ui.label("SCLK channel:");
                ui.add(egui::DragValue::new(sclk_channel).range(0..=7));
            });
            ui.horizontal(|ui| {
                ui.label("MOSI channel:");
                ui.add(egui::DragValue::new(mosi_channel).range(0..=7));
            });
            ui.horizontal(|ui| {
                ui.label("MISO channel:");
                ui.add(egui::DragValue::new(miso_channel).range(0..=7));
            });
            ui.horizontal(|ui| {
                ui.label("CS channel:");
                ui.add(egui::DragValue::new(cs_channel).range(0..=7));
            });
            ui.horizontal(|ui| {
                ui.label("Mode:");
                ui.add(egui::DragValue::new(mode).range(0..=3));
            });
        }
    }
}
