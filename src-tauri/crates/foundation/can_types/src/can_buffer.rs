//! CAN frame ring buffer — overwrites oldest data + version number + incremental cursor read

use std::collections::VecDeque;

use crate::can_frame::CanFrame;

/// CAN frame ring buffer
///
/// - Capacity limit: drops oldest frame when exceeding `max_size`
/// - Version number: monotonically increases on each `push`, used for cross-shard sync
/// - Incremental cursor: `drain_from` supports subscription streams pulling incrementally by `version`
#[derive(Debug)]
pub struct CanBuffer {
    frames: VecDeque<CanFrame>,
    max_size: usize,
    version: u64,
}

impl CanBuffer {
    /// Create a CAN buffer with the specified capacity (pre-allocates at least 8192 frames).
    ///
    /// `max_size` is clamped to a minimum of 1 to avoid degenerate zero-capacity state.
    pub fn new(max_size: usize) -> Self {
        let max_size = max_size.max(1);
        Self {
            frames: VecDeque::with_capacity(max_size.min(8192)),
            max_size,
            version: 0,
        }
    }

    /// Push a frame (drops oldest frame when at capacity)
    pub fn push(&mut self, frame: CanFrame) {
        if self.frames.len() >= self.max_size {
            self.frames.pop_front();
        }
        self.frames.push_back(frame);
        self.version = self.version.wrapping_add(1);
    }

    /// Current version number (monotonically increases on push)
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Get the most recent n frames (returned in time order, oldest first)
    pub fn get_recent(&self, count: usize) -> Vec<CanFrame> {
        let n = count.min(self.frames.len());
        self.frames.iter().rev().take(n).rev().cloned().collect()
    }

    /// Incremental cursor read — for unified sharded streams
    ///
    /// cursor is an absolute sequence number (`version` = cumulative push count). Readable range = `[max(cursor, version-len), version)`.
    /// If cursor has been evicted past, it is shifted forward and counted in `dropped`.
    /// Returns `(items, new_cursor, dropped)`.
    ///
    /// Behavior contract:
    /// - `cursor >= version`: returns all current frames; `new_cursor = cursor` (no backward seek);
    ///   `dropped = cursor - version` (represents logical frames skipped between cursor and current).
    /// - `cursor < version`: reads at most `max` frames from `max(cursor, oldest)`;
    ///   `dropped = start - cursor` (portion of cursor that has been evicted).
    pub fn drain_from(&self, cursor: u64, max: usize) -> (Vec<CanFrame>, u64, u64) {
        let version = self.version;
        // Fully ahead: return current buffer, cursor does not seek backward
        if cursor >= version {
            let items: Vec<CanFrame> = self.frames.iter().cloned().collect();
            return (items, cursor, cursor - version);
        }
        let len = self.frames.len() as u64;
        let oldest = version.saturating_sub(len);
        let start = cursor.max(oldest);
        let dropped = start.saturating_sub(cursor);
        let n = usize::try_from(version - start).unwrap_or(0).min(max);
        let skip = usize::try_from(start.saturating_sub(oldest)).unwrap_or(0);
        let items = self.frames.iter().skip(skip).take(n).cloned().collect();
        (items, start + n as u64, dropped)
    }

    /// Clear the buffer (version number does not reset, continues incrementing to stay in sync with external subscribers)
    pub fn clear(&mut self) {
        self.frames.clear();
        self.version = self.version.wrapping_add(1);
    }

    /// Set maximum capacity (retains most recent frames)
    pub fn set_max_size(&mut self, max_size: usize) {
        self.max_size = max_size.max(1);
        while self.frames.len() > self.max_size {
            self.frames.pop_front();
        }
    }

    /// Number of frames currently in the buffer
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Whether the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Maximum capacity
    pub const fn capacity(&self) -> usize {
        self.max_size
    }
}
