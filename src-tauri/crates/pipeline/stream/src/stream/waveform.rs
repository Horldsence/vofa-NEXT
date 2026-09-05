//! 波形流 — 快照语义 Source (唯一非增量流)

use crate::stream::StreamSource;
use buffer_databuffer::{DataBuffer, WaveformSeriesSelection, WaveformWindow};
use parking_lot::Mutex;
use std::sync::Arc;

/// 波形流 — 快照语义 (唯一非增量流): version 变化即推送最新窗口,
/// 前端按 "最新 seq 胜出" 丢弃乱序旧快照
pub struct WaveformSource {
    buffer: Arc<Mutex<DataBuffer>>,
    last_version: u64,
    view: Option<WaveformViewSpec>,
}

#[derive(Debug, Clone)]
pub struct WaveformViewSpec {
    pub start_ms: f64,
    pub end_ms: f64,
    pub selection: WaveformSeriesSelection,
}

impl WaveformSource {
    pub const fn new(buffer: Arc<Mutex<DataBuffer>>) -> Self {
        Self {
            buffer,
            last_version: 0,
            view: None,
        }
    }

    #[must_use]
    pub const fn with_view(buffer: Arc<Mutex<DataBuffer>>, view: WaveformViewSpec) -> Self {
        Self {
            buffer,
            last_version: 0,
            view: Some(view),
        }
    }
}

impl StreamSource for WaveformSource {
    type Batch = WaveformWindow;

    fn backlog(&mut self) -> usize {
        let buf = self.buffer.lock();
        usize::try_from(buf.version().saturating_sub(self.last_version)).unwrap_or(usize::MAX)
    }

    fn drain(&mut self, max: usize) -> Option<Self::Batch> {
        // 锁内只做窗口快照拷贝 (纯 memcpy, 微秒级); min-max 包络在锁外计算。
        // debug 构建下 70k 点 ×4 通道的包络计算可达 20ms+, 若持锁计算,
        // 摄入热路径 (push_frame) 会被饿死在锁竞争上 → 广播溢出丢帧 → 波形失真
        let (snapshot, version) = {
            let buf = self.buffer.lock();
            let version = buf.version();
            if version == self.last_version {
                return None;
            }
            // 预算感知快照: 窗口超出 L0 覆盖或点数远超预算时, 自动从金字塔层
            // 取真实 min-max 包络 (不变量 2: 示波器语义, 旧数据降质不消失)
            let snapshot = self.view.as_ref().map_or_else(
                || buf.snapshot_all_budget(max),
                |view| buf.snapshot_window_budget(view.start_ms, view.end_ms, &view.selection, max),
            );
            (snapshot, version)
        };
        self.last_version = version;
        Some(snapshot.into_min_max(max))
    }

    fn set_seq(batch: &mut Self::Batch, seq: u64) {
        batch.seq = seq;
    }

    const ACTIVATION_UNIT: usize = 200;
    // 波形 detail 的前端预算上限为 12000；流层必须允许完整传递该预算。
    const MAX_DRAIN: usize = 12_000;
    const SNAPSHOT: bool = true;
}
