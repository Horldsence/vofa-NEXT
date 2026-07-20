//! 传输配置面板 — 串口 / UDP / TCP / 测试数据 / slcan / candleLight

use std::sync::Arc;

use eframe::egui;
use parking_lot::Mutex;
use vofa_next_core::{
    CanBitrate, ConnectionState, FlowControl, Parity, PortInfo, StopBits, TestSignal,
    TransportConfig,
};

use crate::core::{services, AppState};

const BAUD_RATES: [u32; 9] = [
    9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600, 1_000_000,
];

const CAN_BITRATES: [(CanBitrate, &str); 5] = [
    (CanBitrate::Bps100k, "100 kbit/s"),
    (CanBitrate::Bps125k, "125 kbit/s"),
    (CanBitrate::Bps250k, "250 kbit/s"),
    (CanBitrate::Bps500k, "500 kbit/s"),
    (CanBitrate::Bps1m, "1 Mbit/s"),
];

const TEST_SIGNALS: [(TestSignal, &str); 10] = [
    (TestSignal::Sine, "Sine"),
    (TestSignal::Square, "Square"),
    (TestSignal::Triangle, "Triangle"),
    (TestSignal::Sawtooth, "Sawtooth"),
    (TestSignal::Random, "Random"),
    (TestSignal::Dc, "DC"),
    (TestSignal::Chirp, "Chirp"),
    (TestSignal::Steps, "Steps"),
    (TestSignal::Noise, "Noise"),
    (TestSignal::MultiTone, "Multi-tone"),
];

/// 传输类型 (用于 ComboBox 选择, 与 TransportConfig 变体一一对应)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransportKind {
    Serial,
    Udp,
    TcpClient,
    TcpServer,
    TestData,
    Slcan,
    CandleLight,
}

impl TransportKind {
    const ALL: [Self; 7] = [
        Self::Serial,
        Self::Udp,
        Self::TcpClient,
        Self::TcpServer,
        Self::TestData,
        Self::Slcan,
        Self::CandleLight,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Serial => "Serial",
            Self::Udp => "UDP",
            Self::TcpClient => "TCP Client",
            Self::TcpServer => "TCP Server",
            Self::TestData => "Test Data",
            Self::Slcan => "slcan (CAN)",
            Self::CandleLight => "candleLight (CAN)",
        }
    }

    fn of(config: &TransportConfig) -> Self {
        match config {
            TransportConfig::Serial(_) => Self::Serial,
            TransportConfig::Udp(_) => Self::Udp,
            TransportConfig::TcpClient(_) => Self::TcpClient,
            TransportConfig::TcpServer(_) => Self::TcpServer,
            TransportConfig::TestData(_) => Self::TestData,
            TransportConfig::Slcan(_) => Self::Slcan,
            TransportConfig::CandleLight(_) => Self::CandleLight,
        }
    }

    fn default_config(self) -> TransportConfig {
        match self {
            Self::Serial => TransportConfig::Serial(Default::default()),
            Self::Udp => TransportConfig::Udp(Default::default()),
            Self::TcpClient => TransportConfig::TcpClient(Default::default()),
            Self::TcpServer => TransportConfig::TcpServer(Default::default()),
            Self::TestData => TransportConfig::TestData(Default::default()),
            Self::Slcan => TransportConfig::Slcan(Default::default()),
            Self::CandleLight => TransportConfig::CandleLight(Default::default()),
        }
    }
}

pub struct TransportPanel {
    /// 本地可编辑的传输配置 (Connect 时克隆提交)
    config: TransportConfig,
    /// 可用串口列表 (由 list_ports 异步填充)
    ports: Arc<Mutex<Vec<PortInfo>>>,
    /// 是否已触发过首次端口枚举
    ports_loaded: bool,
    /// 最近一次异步操作的结果提示 (错误信息)
    status: Arc<Mutex<Option<String>>>,
}

impl Default for TransportPanel {
    fn default() -> Self {
        Self {
            config: TransportConfig::Serial(Default::default()),
            ports: Arc::new(Mutex::new(Vec::new())),
            ports_loaded: false,
            status: Arc::new(Mutex::new(None)),
        }
    }
}

impl TransportPanel {
    pub fn ui(&mut self, ui: &mut egui::Ui, state: &Arc<AppState>, rt: &tokio::runtime::Runtime) {
        let conn = *state.connection_state.lock();
        let connected = matches!(conn, ConnectionState::Connected);

        // 连接状态由异步任务写入, 定时重绘以便及时反映
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));

        // 类型选择
        let mut kind = TransportKind::of(&self.config);
        egui::ComboBox::from_label("Transport")
            .selected_text(kind.label())
            .show_ui(ui, |ui| {
                for k in TransportKind::ALL {
                    ui.selectable_value(&mut kind, k, k.label());
                }
            });
        if kind != TransportKind::of(&self.config) {
            self.config = kind.default_config();
        }
        ui.add_space(4.0);

        // 按需触发首次端口枚举
        let needs_ports = matches!(
            self.config,
            TransportConfig::Serial(_) | TransportConfig::Slcan(_)
        );
        if needs_ports && !self.ports_loaded {
            self.ports_loaded = true;
            spawn_list_ports(self.ports.clone(), self.status.clone(), rt);
        }

        // 各类型的表单 (克隆 Arc 句柄, 避免与 &mut self.config 冲突)
        let ports = self.ports.clone();
        let status = self.status.clone();
        match &mut self.config {
            TransportConfig::Serial(cfg) => {
                let (p, s) = (ports.clone(), status.clone());
                port_row(ui, &ports, &mut cfg.port_name, || spawn_list_ports(p, s, rt));
                baud_combo(ui, &mut cfg.baud_rate);
                egui::ComboBox::from_label("Data bits")
                    .selected_text(cfg.data_bits.to_string())
                    .show_ui(ui, |ui| {
                        for b in [5u8, 6, 7, 8] {
                            ui.selectable_value(&mut cfg.data_bits, b, b.to_string());
                        }
                    });
                egui::ComboBox::from_label("Parity")
                    .selected_text(format!("{:?}", cfg.parity))
                    .show_ui(ui, |ui| {
                        for p in [Parity::None, Parity::Odd, Parity::Even] {
                            ui.selectable_value(&mut cfg.parity, p, format!("{p:?}"));
                        }
                    });
                egui::ComboBox::from_label("Stop bits")
                    .selected_text(format!("{:?}", cfg.stop_bits))
                    .show_ui(ui, |ui| {
                        for s in [StopBits::One, StopBits::Two] {
                            ui.selectable_value(&mut cfg.stop_bits, s, format!("{s:?}"));
                        }
                    });
                egui::ComboBox::from_label("Flow control")
                    .selected_text(format!("{:?}", cfg.flow_control))
                    .show_ui(ui, |ui| {
                        for f in [FlowControl::None, FlowControl::Software, FlowControl::Hardware] {
                            ui.selectable_value(&mut cfg.flow_control, f, format!("{f:?}"));
                        }
                    });
            }
            TransportConfig::Udp(cfg) => {
                text_row(ui, "Local addr", &mut cfg.local_addr);
                drag_row(ui, "Local port", &mut cfg.local_port, 0..=65535);
                text_row(ui, "Remote addr", &mut cfg.remote_addr);
                drag_row(ui, "Remote port", &mut cfg.remote_port, 0..=65535);
            }
            TransportConfig::TcpClient(cfg) => {
                text_row(ui, "Host", &mut cfg.host);
                drag_row(ui, "Port", &mut cfg.port, 0..=65535);
            }
            TransportConfig::TcpServer(cfg) => {
                text_row(ui, "Listen addr", &mut cfg.listen_addr);
                drag_row(ui, "Listen port", &mut cfg.listen_port, 0..=65535);
            }
            TransportConfig::TestData(cfg) => {
                drag_row(ui, "Channels", &mut cfg.channels, 1..=64);
                ui.horizontal(|ui| {
                    ui.label("Sample rate");
                    ui.add(
                        egui::DragValue::new(&mut cfg.sample_rate)
                            .range(1.0..=100_000.0)
                            .suffix(" Hz"),
                    );
                });
                egui::ComboBox::from_label("Signal")
                    .selected_text(signal_label(cfg.signal))
                    .show_ui(ui, |ui| {
                        for (sig, label) in TEST_SIGNALS {
                            ui.selectable_value(&mut cfg.signal, sig, label);
                        }
                    });
            }
            TransportConfig::Slcan(cfg) => {
                let (p, s) = (ports.clone(), status.clone());
                port_row(ui, &ports, &mut cfg.port_name, || spawn_list_ports(p, s, rt));
                baud_combo(ui, &mut cfg.baud_rate);
                can_bitrate_combo(ui, &mut cfg.can_bitrate);
            }
            TransportConfig::CandleLight(cfg) => {
                drag_row(ui, "USB bus", &mut cfg.bus, 0..=255);
                drag_row(ui, "USB address", &mut cfg.address, 0..=255);
                drag_row(ui, "CAN channel", &mut cfg.channel, 0..=1);
                can_bitrate_combo(ui, &mut cfg.can_bitrate);
            }
        }

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);

        // 连接状态 + 操作按钮
        ui.horizontal(|ui| {
            let (label, color) = match conn {
                ConnectionState::Disconnected => ("Disconnected", egui::Color32::GRAY),
                ConnectionState::Connecting => ("Connecting…", egui::Color32::YELLOW),
                ConnectionState::Connected => ("Connected", egui::Color32::from_rgb(80, 200, 120)),
                ConnectionState::Error => ("Error", egui::Color32::LIGHT_RED),
            };
            ui.colored_label(color, "●");
            ui.label(label);
        });
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            let connect = ui.add_enabled(!connected, egui::Button::new("Connect"));
            if connect.clicked() {
                let state = state.clone();
                let config = self.config.clone();
                let status = self.status.clone();
                rt.spawn(async move {
                    let result = services::open_transport(&state, config).await;
                    *status.lock() = result.err().map(|e| e.to_string());
                });
            }

            let disconnect = ui.add_enabled(connected, egui::Button::new("Disconnect"));
            if disconnect.clicked() {
                let state = state.clone();
                let status = self.status.clone();
                rt.spawn(async move {
                    let result = services::close_transport(&state).await;
                    *status.lock() = result.err().map(|e| e.to_string());
                });
            }
        });

        // TestData: 额外提供启动/停止生成按钮
        if matches!(self.config, TransportConfig::TestData(_)) {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(connected, egui::Button::new("Start test data"))
                    .clicked()
                {
                    let state = state.clone();
                    let status = self.status.clone();
                    rt.spawn(async move {
                        let result = services::start_test_data(&state).await;
                        *status.lock() = result.err().map(|e| e.to_string());
                    });
                }
                if ui
                    .add_enabled(connected, egui::Button::new("Stop test data"))
                    .clicked()
                {
                    let state = state.clone();
                    let status = self.status.clone();
                    rt.spawn(async move {
                        let result = services::stop_test_data(&state).await;
                        *status.lock() = result.err().map(|e| e.to_string());
                    });
                }
            });
        }

        if let Some(msg) = self.status.lock().as_ref() {
            ui.add_space(4.0);
            ui.colored_label(egui::Color32::LIGHT_RED, msg);
        }
    }
}

/// 异步刷新串口列表
fn spawn_list_ports(
    ports: Arc<Mutex<Vec<PortInfo>>>,
    status: Arc<Mutex<Option<String>>>,
    rt: &tokio::runtime::Runtime,
) {
    rt.spawn(async move {
        match services::list_ports().await {
            Ok(list) => *ports.lock() = list,
            Err(e) => *status.lock() = Some(e.to_string()),
        }
    });
}

fn signal_label(signal: TestSignal) -> &'static str {
    TEST_SIGNALS
        .iter()
        .find(|(s, _)| *s == signal)
        .map(|(_, l)| *l)
        .unwrap_or("?")
}

/// 端口选择行: ComboBox + Refresh 按钮
fn port_row(
    ui: &mut egui::Ui,
    ports: &Arc<Mutex<Vec<PortInfo>>>,
    selected: &mut String,
    on_refresh: impl FnOnce(),
) {
    ui.horizontal(|ui| {
        let selected_text = if selected.is_empty() {
            "Select port…".to_string()
        } else {
            selected.clone()
        };
        egui::ComboBox::from_label("Port")
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                for port in ports.lock().iter() {
                    ui.selectable_value(
                        selected,
                        port.name.clone(),
                        format!("{} ({})", port.name, port.port_type),
                    );
                }
            });
        if ui.button("↻").on_hover_text("Refresh ports").clicked() {
            on_refresh();
        }
    });
}

fn baud_combo(ui: &mut egui::Ui, baud_rate: &mut u32) {
    egui::ComboBox::from_label("Baud rate")
        .selected_text(baud_rate.to_string())
        .show_ui(ui, |ui| {
            for b in BAUD_RATES {
                ui.selectable_value(baud_rate, b, b.to_string());
            }
        });
}

fn can_bitrate_combo(ui: &mut egui::Ui, bitrate: &mut CanBitrate) {
    let label = CAN_BITRATES
        .iter()
        .find(|(b, _)| b == bitrate)
        .map(|(_, l)| *l)
        .unwrap_or("?");
    egui::ComboBox::from_label("CAN bitrate")
        .selected_text(label)
        .show_ui(ui, |ui| {
            for (b, l) in CAN_BITRATES {
                ui.selectable_value(bitrate, b, l);
            }
        });
}

fn text_row(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(format!("{label}:"));
        ui.add(egui::TextEdit::singleline(value).desired_width(140.0));
    });
}

fn drag_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut impl egui::emath::Numeric,
    range: std::ops::RangeInclusive<i64>,
) {
    ui.horizontal(|ui| {
        ui.label(format!("{label}:"));
        ui.add(egui::DragValue::new(value).range(range));
    });
}
