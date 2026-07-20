//! 中央停靠区 — egui_dock 的 Tab 定义与 TabViewer 实现

use std::collections::HashMap;
use std::sync::Arc;

use eframe::egui;
use egui_dock::TabViewer;

use crate::core::AppState;
use crate::ui::displays::{self, DataTabState};
use crate::ui::node_editor::{self, ControlTabState};

/// 停靠区中的页签
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tab {
    /// 控制页签 — 承载控件/节点图 (每个 tab 对应 core 中一个独立编译图)
    Control { id: u64 },
    /// 数据页签 — 承载波形/原始数据/CAN/逻辑/频谱等数据视图
    Data { kind: DataKind, id: u64 },
}

impl Tab {
    pub fn control(id: u64) -> Self {
        Self::Control { id }
    }

    pub fn data(kind: DataKind, id: u64) -> Self {
        Self::Data { kind, id }
    }
}

/// 数据页签类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataKind {
    Waveform,
    RawData,
    Can,
    Logic,
    Decoded,
    Spectrum,
}

impl DataKind {
    /// 全部类型 (供菜单遍历)
    pub const ALL: [DataKind; 6] = [
        Self::Waveform,
        Self::RawData,
        Self::Can,
        Self::Logic,
        Self::Decoded,
        Self::Spectrum,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Waveform => "Waveform",
            Self::RawData => "Raw Data",
            Self::Can => "CAN",
            Self::Logic => "Logic",
            Self::Decoded => "Decoded",
            Self::Spectrum => "Spectrum",
        }
    }
}

/// DockArea 的 TabViewer — Control 页签渲染节点编辑器, Data 页签渲染数据视图
pub struct DockViewer<'a> {
    /// Control 页签 id → 节点编辑器状态
    pub control_tabs: &'a mut HashMap<u64, ControlTabState>,
    /// Data 页签 id → 数据显示状态
    pub data_tabs: &'a mut HashMap<u64, DataTabState>,
    /// 后端共享状态 (节点体展示 output_snapshot)
    pub state: &'a Arc<AppState>,
    /// Tokio 运行时 (spawn 图同步)
    pub rt: &'a tokio::runtime::Runtime,
}

impl TabViewer for DockViewer<'_> {
    type Tab = Tab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        match tab {
            Tab::Control { id } => format!("Control {id}").into(),
            Tab::Data { kind, id } => format!("{} {id}", kind.label()).into(),
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match tab {
            Tab::Control { id } => {
                let tab_state = self
                    .control_tabs
                    .entry(*id)
                    .or_insert_with(|| ControlTabState::new(*id));
                node_editor::show_node_editor(ui, tab_state, self.state, self.rt, *id);
            }
            Tab::Data { kind, id } => {
                let tab_state = self
                    .data_tabs
                    .entry(*id)
                    .or_insert_with(DataTabState::new);
                match kind {
                    DataKind::Waveform => {
                        displays::waveform::show(ui, self.state, &mut tab_state.waveform)
                    }
                    DataKind::RawData => {
                        displays::raw_data::show(ui, self.state, self.rt, &mut tab_state.raw_data)
                    }
                    DataKind::Can => displays::can::show(ui, self.state),
                    DataKind::Logic => displays::logic::show(ui, self.state),
                    DataKind::Decoded => displays::logic::show_decoded_events(ui, self.state),
                    DataKind::Spectrum => {
                        displays::spectrum::show(ui, self.state, &mut tab_state.spectrum)
                    }
                }
            }
        }
    }

    fn scroll_bars(&self, _tab: &Self::Tab) -> [bool; 2] {
        // 节点编辑器自行平移/缩放, 数据视图自行管理滚动区
        [false, false]
    }
}
