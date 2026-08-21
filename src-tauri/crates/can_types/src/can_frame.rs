//! CAN frame, direction, bitrate, filter, batch, and candleLight device info

use serde::{Deserialize, Serialize};

// ============ CAN Frame Base Types ============

/// CAN frame direction
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum CanDirection {
    #[default]
    Rx,
    Tx,
}

/// CAN frame — normalized CAN data model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanFrame {
    pub timestamp: u64,
    pub id: u32,
    pub extended: bool,
    pub rtr: bool,
    pub dlc: u8,
    pub data: Vec<u8>,
    pub direction: CanDirection,
}

impl CanFrame {
    /// Construct a CAN frame with given timestamp, ID, direction and data.
    ///
    /// `data` exceeding 8 bytes is truncated to match the CAN DLC limit.
    #[allow(clippy::cast_possible_truncation)]
    pub fn new(timestamp: u64, id: u32, data: Vec<u8>, direction: CanDirection) -> Self {
        let dlc = u8::try_from(data.len().min(8)).expect("dlc is capped at 8");
        let data = data.into_iter().take(dlc as usize).collect();
        Self {
            timestamp,
            id,
            extended: false,
            rtr: false,
            dlc,
            data,
            direction,
        }
    }

    /// Number of data bytes (based on DLC)
    pub const fn data_len(&self) -> usize {
        self.dlc as usize
    }
}

/// CAN bitrate
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CanBitrate {
    Bps100k,
    Bps125k,
    Bps250k,
    Bps500k,
    Bps1m,
}

impl CanBitrate {
    /// Returns the bitrate value in bps
    pub const fn bps(&self) -> u32 {
        match self {
            Self::Bps100k => 100_000,
            Self::Bps125k => 125_000,
            Self::Bps250k => 250_000,
            Self::Bps500k => 500_000,
            Self::Bps1m => 1_000_000,
        }
    }

    /// slcan baudrate command character (Lawicel protocol)
    pub const fn slcan_cmd(&self) -> &'static str {
        match self {
            Self::Bps100k => "S3",
            Self::Bps125k => "S4",
            Self::Bps250k => "S5",
            Self::Bps500k => "S6",
            Self::Bps1m => "S8",
        }
    }
}

// ============ CAN Filter and Batch ============

/// CAN filter configuration — controls which frames pass through via ID bitmask
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanFilter {
    /// Whether the filter is enabled
    pub enabled: bool,
    /// Standard frame ID mask (only keeps bits where the mask is 1 in the lower 11 bits)
    pub id_mask_std: u16,
    /// Extended frame ID mask (only keeps bits where the mask is 1 in the lower 29 bits)
    pub id_mask_ext: u32,
}

impl CanFilter {
    /// Check if a frame matches the filter conditions
    ///
    /// When disabled, always matches; when enabled, applies ID mask separately for standard/extended frames.
    /// Frames with `(frame.id & mask) != 0` are considered a match.
    pub const fn matches(&self, frame: &CanFrame) -> bool {
        if !self.enabled {
            return true;
        }
        if frame.extended {
            (frame.id & self.id_mask_ext) != 0
        } else {
            (frame.id & self.id_mask_std as u32) != 0
        }
    }
}

/// CAN frame filter matcher — direction + whitelist/blacklist
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CanFrameFilter {
    /// Whether to receive Rx frames only
    pub rx_only: bool,
    /// Whether to receive Tx frames only
    pub tx_only: bool,
    /// ID whitelist (empty means no restriction)
    pub id_whitelist: Vec<u32>,
    /// ID blacklist
    pub id_blacklist: Vec<u32>,
}

impl CanFrameFilter {
    /// Check if a frame matches the filter conditions
    pub fn matches(&self, frame: &CanFrame) -> bool {
        if self.rx_only && frame.direction != CanDirection::Rx {
            return false;
        }
        if self.tx_only && frame.direction != CanDirection::Tx {
            return false;
        }
        if !self.id_whitelist.is_empty() && !self.id_whitelist.contains(&frame.id) {
            return false;
        }
        if self.id_blacklist.contains(&frame.id) {
            return false;
        }
        true
    }
}

/// CAN frame batch (for bulk transfer) — monotonically increasing `seq`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanFrameBatch {
    pub seq: u64,
    pub frames: Vec<CanFrame>,
}

impl CanFrameBatch {
    /// Construct an empty batch
    pub const fn new(seq: u64) -> Self {
        Self {
            seq,
            frames: Vec::new(),
        }
    }

    /// Number of frames
    pub const fn len(&self) -> usize {
        self.frames.len()
    }

    /// Whether the batch is empty
    pub const fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}

/// candleLight device information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandleDeviceInfo {
    pub bus: u8,
    pub address: u8,
    pub vid: u16,
    pub pid: u16,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub serial_number: Option<String>,
}
