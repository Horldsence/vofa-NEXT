//! CAN load statistics — sliding time window + bit-stuffing estimate + per-ID distribution
//!
//! Main API:
//! - [`CanLoadStats::new`] — create the stats collector
//! - [`CanLoadStats::push`] — push a frame
//! - [`CanLoadStats::sample_history`] — sample history
//! - [`CanLoadStats::snapshot`] — generate snapshot
//!
//! ## Bit Count Estimation Formula (with 1.2x bit-stuffing factor)
//!
//! - Standard frame: `(47 + 8×DLC) × 1.2`
//! - Extended frame: `(67 + 8×DLC) × 1.2`

use std::cmp::Reverse;
use std::collections::{HashMap, VecDeque};

use crate::can_frame::CanFrame;
use crate::can_load_types::{
    CanIdLoadHistory, CanIdLoadStats, CanLoadHistoryPoint, CanLoadSnapshot,
};

/// CAN load statistics — based on sliding time window
///
/// On each frame push, automatically evicts expired samples outside the window and maintains:
/// - Total bits within window (for load ratio calculation)
/// - Per-ID frame count / bit count / byte count statistics
/// - Recent N sample points of load ratio history (for frontend timeline rendering)
pub struct CanLoadStats {
    samples: VecDeque<(u64, u32, u32, bool, u8)>,
    window_us: u64,
    total_bits: u64,
    total_bytes: u64,
    per_id: HashMap<(u32, bool), CanIdLoadStats>,
    history: VecDeque<CanLoadHistoryPoint>,
    per_id_history: HashMap<(u32, bool), VecDeque<CanLoadHistoryPoint>>,
    history_capacity: usize,
}

impl CanLoadStats {
    /// Create a load statistics collector
    ///
    /// - `window_us`: sliding window size in microseconds, e.g. `1_000_000` = 1 second
    /// - `history_capacity`: maximum number of historical sample points to retain (for timeline)
    pub fn new(window_us: u64, history_capacity: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(4096),
            window_us: window_us.max(1),
            total_bits: 0,
            total_bytes: 0,
            per_id: HashMap::new(),
            history: VecDeque::with_capacity(history_capacity),
            per_id_history: HashMap::new(),
            history_capacity,
        }
    }

    /// Set sliding window size in microseconds — actively evicts expired samples after shrinking
    pub fn set_window_us(&mut self, window_us: u64) {
        self.window_us = window_us.max(1);
        if let Some(&(ts, _, _, _, _)) = self.samples.back() {
            self.evict_expired(ts);
        }
    }

    /// Current window size in microseconds
    pub const fn window_us(&self) -> u64 {
        self.window_us
    }

    /// Push a frame, update window statistics
    pub fn push(&mut self, frame: &CanFrame) {
        let bits = frame_bits(frame);
        self.evict_expired(frame.timestamp);
        self.samples
            .push_back((frame.timestamp, bits, frame.id, frame.extended, frame.dlc));
        self.total_bits += u64::from(bits);
        self.total_bytes += u64::from(frame.dlc);
        let entry = self
            .per_id
            .entry((frame.id, frame.extended))
            .or_insert_with(|| CanIdLoadStats {
                id: frame.id,
                extended: frame.extended,
                frame_count: 0,
                total_bits: 0,
                total_bytes: 0,
            });
        entry.frame_count += 1;
        entry.total_bits += u64::from(bits);
        entry.total_bytes += u64::from(frame.dlc);
    }

    /// Sample current load ratio and push to history (called by frontend at fixed interval).
    /// Also samples per-ID load ratio for each ID currently in the window.
    #[allow(clippy::cast_precision_loss)]
    pub fn sample_history(&mut self, bitrate: u32, now_us: u64) {
        self.evict_expired(now_us);
        let load_ratio = self.load_ratio(bitrate);
        let fps = if self.window_us > 0 {
            (self.samples.len() as f64) * 1_000_000.0 / self.window_us as f64
        } else {
            0.0
        };
        let point = CanLoadHistoryPoint {
            timestamp: now_us,
            load_ratio,
            fps,
        };
        self.history.push_back(point);
        while self.history.len() > self.history_capacity {
            self.history.pop_front();
        }

        let window_bits = if self.window_us > 0 && bitrate > 0 {
            (self.window_us as f64 / 1_000_000.0) * f64::from(bitrate)
        } else {
            0.0
        };
        for ((id, ext), entry) in &self.per_id {
            let id_load = if window_bits > 0.0 {
                entry.total_bits as f64 / window_bits
            } else {
                0.0
            };
            let id_point = CanLoadHistoryPoint {
                timestamp: now_us,
                load_ratio: id_load,
                fps: 0.0,
            };
            let hist = self
                .per_id_history
                .entry((*id, *ext))
                .or_insert_with(|| VecDeque::with_capacity(self.history_capacity));
            hist.push_back(id_point);
            while hist.len() > self.history_capacity {
                hist.pop_front();
            }
        }
    }

    /// Current load ratio (0.0 - 1.0+, can exceed 1.0 indicating overload)
    #[allow(clippy::cast_precision_loss)]
    pub fn load_ratio(&self, bitrate: u32) -> f64 {
        if self.window_us == 0 || bitrate == 0 {
            return 0.0;
        }
        let window_bits = (self.window_us as f64 / 1_000_000.0) * f64::from(bitrate);
        if window_bits <= 0.0 {
            return 0.0;
        }
        self.total_bits as f64 / window_bits
    }

    /// Current frame rate (frames per second)
    #[allow(clippy::cast_precision_loss)]
    pub fn fps(&self) -> f64 {
        if self.window_us == 0 {
            return 0.0;
        }
        (self.samples.len() as f64) * 1_000_000.0 / self.window_us as f64
    }

    /// Generate current snapshot (includes history samples + per-id sorting + per_id_history)
    pub fn snapshot(&self, bitrate: u32) -> CanLoadSnapshot {
        let mut per_id: Vec<CanIdLoadStats> = self.per_id.values().cloned().collect();
        per_id.sort_by_key(|s| Reverse(s.total_bits));
        let mut per_id_history: Vec<CanIdLoadHistory> = self
            .per_id_history
            .iter()
            .map(|((id, ext), hist)| CanIdLoadHistory {
                id: *id,
                extended: *ext,
                history: hist.iter().copied().collect(),
            })
            .collect();
        per_id_history.sort_by(|a, b| {
            let a_bits = self
                .per_id
                .get(&(a.id, a.extended))
                .map_or(0, |s| s.total_bits);
            let b_bits = self
                .per_id
                .get(&(b.id, b.extended))
                .map_or(0, |s| s.total_bits);
            b_bits.cmp(&a_bits)
        });
        CanLoadSnapshot {
            window_us: self.window_us,
            frame_count: self.samples.len() as u64,
            total_bits: self.total_bits,
            total_bytes: self.total_bytes,
            load_ratio: self.load_ratio(bitrate),
            history: self.history.iter().copied().collect(),
            per_id,
            per_id_history,
        }
    }

    /// Clear all statistics
    pub fn clear(&mut self) {
        self.samples.clear();
        self.total_bits = 0;
        self.total_bytes = 0;
        self.per_id.clear();
        self.history.clear();
        self.per_id_history.clear();
    }

    /// Evict expired samples outside the window (using `now_us` as reference)
    fn evict_expired(&mut self, now_us: u64) {
        let cutoff = now_us.saturating_sub(self.window_us);
        while let Some(&(ts, bits, id, ext, dlc)) = self.samples.front() {
            if ts < cutoff {
                self.samples.pop_front();
                self.total_bits = self.total_bits.saturating_sub(u64::from(bits));
                self.total_bytes = self.total_bytes.saturating_sub(u64::from(dlc));
                if let Some(entry) = self.per_id.get_mut(&(id, ext)) {
                    entry.frame_count = entry.frame_count.saturating_sub(1);
                    entry.total_bits = entry.total_bits.saturating_sub(u64::from(bits));
                    entry.total_bytes = entry.total_bytes.saturating_sub(u64::from(dlc));
                    if entry.frame_count == 0 {
                        self.per_id.remove(&(id, ext));
                        self.per_id_history.remove(&(id, ext));
                    }
                }
            } else {
                break;
            }
        }
    }
}

/// CAN frame bit count estimation (with 1.2x bit-stuffing factor)
///
/// Standard frame: SOF(1) + ID(11) + RTR(1) + IDE(1) + r0(1) + DLC(4) + Data(8×DLC) + CRC(15) + CRCdel(1) + ACK(1) + ACKdel(1) + EOF(7) + IFS(3) = 47 + 8×DLC
/// Extended frame: SOF(1) + ID-A(11) + SRR(1) + IDE(1) + ID-B(18) + RTR(1) + r1(1) + r0(1) + DLC(4) + Data(8×DLC) + CRC(15) + CRCdel(1) + ACK(1) + ACKdel(1) + EOF(7) + IFS(3) = 67 + 8×DLC
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
pub fn frame_bits(frame: &CanFrame) -> u32 {
    let base = if frame.extended { 67 } else { 47 };
    let raw = base + 8 * u32::from(frame.dlc);
    (f64::from(raw) * 1.2) as u32
}
