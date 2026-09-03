//! CAN load statistics pure data types — snapshots, history samples, per-ID distribution
//!
//! Implementation/calculation logic is in [`crate::can_load_stats`].

use serde::{Deserialize, Serialize};

/// Load statistics snapshot for a single ID
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CanIdLoadStats {
    pub id: u32,
    pub extended: bool,
    pub frame_count: u64,
    /// Total bits (including bit-stuffing estimate)
    pub total_bits: u64,
    /// Total bytes (sum of DLC)
    pub total_bytes: u64,
}

/// CAN load statistics snapshot — computed from sliding window
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanLoadSnapshot {
    /// Window size in microseconds
    pub window_us: u64,
    /// Total frame count within the window
    pub frame_count: u64,
    /// Total bits within the window (including bit-stuffing estimate)
    pub total_bits: u64,
    /// Total bytes within the window
    pub total_bytes: u64,
    /// Current load ratio (0.0 - 1.0+, can exceed 1.0 indicating overload)
    pub load_ratio: f64,
    /// Time series samples (recent load ratio history, used for line charts)
    pub history: Vec<CanLoadHistoryPoint>,
    /// Per-ID load distribution (sorted by `total_bits` descending)
    pub per_id: Vec<CanIdLoadStats>,
    /// Per-ID load ratio history (for overlaid timeline display)
    pub per_id_history: Vec<CanIdLoadHistory>,
}

/// Load ratio history for a single ID (for timeline overlay)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanIdLoadHistory {
    pub id: u32,
    pub extended: bool,
    pub history: Vec<CanLoadHistoryPoint>,
}

/// Load history sample point
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
// Note: does not derive Eq because f64 fields make it impossible
pub struct CanLoadHistoryPoint {
    /// Timestamp in microseconds
    pub timestamp: u64,
    /// Load ratio (0.0 - 1.0+)
    pub load_ratio: f64,
    /// Frame rate (frames per second)
    pub fps: f64,
}
