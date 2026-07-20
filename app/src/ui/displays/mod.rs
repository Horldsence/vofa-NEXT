//! # displays — Data 页签的数据可视化组件
//!
//! 每个模块对应一种 [`crate::ui::dock::DataKind`],
//! 每帧从 `core::AppState` 的共享缓冲区读取数据并渲染。
//! 需要跨帧持久化的 UI 状态 (运行/停止、滚动缓冲等) 收拢在 [`DataTabState`] 中,
//! 由 `VofaApp` 按页签 id 持有。

pub mod can;
pub mod logic;
pub mod raw_data;
pub mod spectrum;
pub mod waveform;

/// Data 页签的持久化 UI 状态 (按页签 id 索引, 页签关闭时回收)
pub struct DataTabState {
    /// 波形显示状态 (Run/Stop、通道可见性)
    pub waveform: waveform::WaveformState,
    /// 原始数据滚动缓冲 (Hex 视图)
    pub raw_data: raw_data::RawDataState,
    /// 频谱显示状态 (当前选中的 SpectrumSink)
    pub spectrum: spectrum::SpectrumState,
}

impl DataTabState {
    pub fn new() -> Self {
        Self {
            waveform: waveform::WaveformState::new(),
            raw_data: raw_data::RawDataState::new(),
            spectrum: spectrum::SpectrumState::new(),
        }
    }
}

impl Default for DataTabState {
    fn default() -> Self {
        Self::new()
    }
}
