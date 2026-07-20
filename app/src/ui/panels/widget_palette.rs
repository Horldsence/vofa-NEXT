//! 控件库面板 — 分组展示可用控件, 点击添加到当前 Control 页签画布

use eframe::egui;

struct WidgetEntry {
    id: &'static str,
    icon: &'static str,
    name: &'static str,
    description: &'static str,
}

struct WidgetGroup {
    name: &'static str,
    entries: &'static [WidgetEntry],
}

const INPUT: &[WidgetEntry] = &[
    WidgetEntry {
        id: "knob",
        icon: "🎛",
        name: "Knob",
        description: "Rotary value input",
    },
    WidgetEntry {
        id: "button",
        icon: "🔘",
        name: "Button",
        description: "Press/release value",
    },
    WidgetEntry {
        id: "radio",
        icon: "📻",
        name: "Radio",
        description: "Exclusive option select",
    },
    WidgetEntry {
        id: "checkbox",
        icon: "☑",
        name: "Checkbox",
        description: "On/off toggle",
    },
    WidgetEntry {
        id: "slider",
        icon: "🎚",
        name: "Slider",
        description: "Linear value input",
    },
    WidgetEntry {
        id: "label",
        icon: "🏷",
        name: "Label",
        description: "Static or channel text",
    },
];

const DISPLAY: &[WidgetEntry] = &[
    WidgetEntry {
        id: "waveform",
        icon: "📈",
        name: "Waveform",
        description: "Multi-channel time plot",
    },
    WidgetEntry {
        id: "pie_chart",
        icon: "🥧",
        name: "PieChart",
        description: "Proportion segments",
    },
    WidgetEntry {
        id: "gauge",
        icon: "🧭",
        name: "Gauge",
        description: "Analog dial indicator",
    },
    WidgetEntry {
        id: "led",
        icon: "💡",
        name: "LED",
        description: "Boolean status light",
    },
    WidgetEntry {
        id: "number_display",
        icon: "🔢",
        name: "NumberDisplay",
        description: "Numeric readout",
    },
    WidgetEntry {
        id: "spectrum",
        icon: "📊",
        name: "Spectrum",
        description: "FFT frequency plot",
    },
    WidgetEntry {
        id: "frame_decoder",
        icon: "🧩",
        name: "FrameDecoder",
        description: "Byte-stream frame parser",
    },
    WidgetEntry {
        id: "table_view",
        icon: "📋",
        name: "TableView",
        description: "Tabular channel data",
    },
    WidgetEntry {
        id: "command",
        icon: "⌨",
        name: "Command",
        description: "Command sender",
    },
];

const MATH: &[WidgetEntry] = &[
    WidgetEntry {
        id: "math",
        icon: "➗",
        name: "Math",
        description: "Expression on channels",
    },
    WidgetEntry {
        id: "filter",
        icon: "🌀",
        name: "Filter",
        description: "FIR/IIR signal filter",
    },
];

const SINK: &[WidgetEntry] = &[
    WidgetEntry {
        id: "raw_data_sink",
        icon: "💾",
        name: "RawDataSink",
        description: "Record raw bytes",
    },
    WidgetEntry {
        id: "custom_sink",
        icon: "🧰",
        name: "CustomSink",
        description: "User-defined output",
    },
];

const GROUPS: &[WidgetGroup] = &[
    WidgetGroup {
        name: "Input",
        entries: INPUT,
    },
    WidgetGroup {
        name: "Display",
        entries: DISPLAY,
    },
    WidgetGroup {
        name: "Math",
        entries: MATH,
    },
    WidgetGroup {
        name: "Sink",
        entries: SINK,
    },
];

#[derive(Default)]
pub struct WidgetPalettePanel {
    /// 当前选中的控件 id
    selected: Option<&'static str>,
    /// 待添加到画布的控件 id (由 VofaApp 消费)
    pending_add: Option<&'static str>,
}

impl WidgetPalettePanel {
    /// 取出本帧点击待添加的控件 id (消费一次)
    pub fn take_pending_add(&mut self) -> Option<&'static str> {
        self.pending_add.take()
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.small("Click to add the widget to the active Control tab.");
        ui.add_space(4.0);

        for group in GROUPS {
            egui::CollapsingHeader::new(group.name)
                .default_open(true)
                .show(ui, |ui| {
                    for entry in group.entries {
                        let is_selected = self.selected == Some(entry.id);
                        let row = ui.selectable_label(
                            is_selected,
                            format!("{}  {}", entry.icon, entry.name),
                        );
                        if row.clicked() {
                            self.selected = Some(entry.id);
                            self.pending_add = Some(entry.id);
                        }
                        ui.indent(entry.id, |ui| {
                            ui.add_space(-4.0);
                            ui.small(entry.description);
                        });
                        ui.add_space(2.0);
                    }
                });
        }
    }
}
