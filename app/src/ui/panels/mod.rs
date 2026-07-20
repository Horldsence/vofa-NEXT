//! # panels — 侧栏面板 (本地可编辑状态 + services 接线)
//!
//! - [`transport`][]: 传输配置面板 (串口/网络/CAN/测试数据)
//! - [`protocol`][]: 协议配置面板 (FireWater / JustFloat / RawData / CAN / 逻辑解码)
//! - [`widget_palette`][]: 控件库面板 (点击添加到活动 Control 页签画布)

mod protocol;
mod settings;
mod transport;
mod widget_palette;

pub use protocol::ProtocolPanel;
pub use settings::SettingsPanel;
pub use transport::TransportPanel;
pub use widget_palette::WidgetPalettePanel;

/// 侧栏全部面板实例, 由 `VofaApp` 持有并传给 sidebar 分发
#[derive(Default)]
pub struct Panels {
    pub transport: TransportPanel,
    pub protocol: ProtocolPanel,
    pub widget_palette: WidgetPalettePanel,
    pub settings: SettingsPanel,
}
