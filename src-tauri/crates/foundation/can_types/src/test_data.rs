//! CAN frame test data generator — for constructing CAN frame streams in unit/integration tests

use crate::can_frame::{CanDirection, CanFrame};

/// CAN frame test data generator
///
/// Provides various patterns for generating [`CanFrame`] sequences, used for testing CAN buffer, load statistics and frame filtering.
pub struct CanFrameTestData;

#[allow(clippy::cast_possible_truncation)]
impl CanFrameTestData {
    /// Generate the specified number of standard frames, IDs starting from `base_id` and incrementing
    ///
    /// Each frame carries 8 bytes of data, the first byte equals the frame sequence number (0..count), rest are 0.
    /// Timestamp starts at 0, with 1000 microseconds between each frame.
    pub fn standard_frames(base_id: u32, count: usize) -> Vec<CanFrame> {
        (0..count)
            .map(|i| CanFrame {
                timestamp: i as u64 * 1000,
                id: base_id + i as u32,
                extended: false,
                rtr: false,
                dlc: 8,
                data: {
                    let mut d = vec![0u8; 8];
                    d[0] = i as u8;
                    d
                },
                direction: CanDirection::Rx,
            })
            .collect()
    }

    /// Generate the specified number of extended frames, IDs starting from `base_id` and incrementing
    pub fn extended_frames(base_id: u32, count: usize) -> Vec<CanFrame> {
        (0..count)
            .map(|i| CanFrame {
                timestamp: i as u64 * 1000,
                id: base_id + i as u32,
                extended: true,
                rtr: false,
                dlc: 8,
                data: {
                    let mut d = vec![0u8; 8];
                    d[0] = i as u8;
                    d
                },
                direction: CanDirection::Rx,
            })
            .collect()
    }

    /// Generate repeating frames with the same ID and data pattern
    ///
    /// All frames share the same `id`, `data` and `extended` flag.
    /// Timestamp starts at 0 with 1000 microseconds between each frame.
    pub fn repeating(id: u32, data: Vec<u8>, extended: bool, count: usize) -> Vec<CanFrame> {
        let dlc = data.len().min(8) as u8;
        let payload = data[..dlc as usize].to_vec();
        (0..count)
            .map(|i| CanFrame {
                timestamp: i as u64 * 1000,
                id,
                extended,
                rtr: false,
                dlc,
                data: payload.clone(),
                direction: CanDirection::Rx,
            })
            .collect()
    }

    /// Generate cycling frames across multiple IDs
    ///
    /// Iterates over the `ids` list repeatedly to generate frames, each frame carrying `data_len` bytes of data.
    /// Timestamp starts at 0 with 1000 microseconds between each frame.
    pub fn cycling(ids: &[u32], data_len: u8, count: usize) -> Vec<CanFrame> {
        let dlc = data_len.min(8);
        (0..count)
            .map(|i| {
                let id = ids[i % ids.len()];
                let mut data = vec![0; dlc as usize];
                data[0] = i as u8;
                CanFrame {
                    timestamp: i as u64 * 1000,
                    id,
                    extended: false,
                    rtr: false,
                    dlc,
                    data,
                    direction: CanDirection::Rx,
                }
            })
            .collect()
    }

    /// 生成一帧用于负载测试(带时间戳)
    ///
    /// 创建 `dlc` 字节的空数据帧,适合推入 [`crate::CanLoadStats`]。
    pub fn load_frame(id: u32, dlc: u8, timestamp_us: u64) -> CanFrame {
        let dlc = dlc.min(8);
        CanFrame {
            timestamp: timestamp_us,
            id,
            extended: false,
            rtr: false,
            dlc,
            data: vec![0; dlc as usize],
            direction: CanDirection::Rx,
        }
    }

    /// 生成带指定数据模式的帧
    ///
    /// `data` 长度超过 8 时自动截断。
    pub fn with_data(id: u32, data: Vec<u8>, extended: bool) -> CanFrame {
        let dlc = data.len().min(8) as u8;
        let payload = data[..dlc as usize].to_vec();
        CanFrame {
            timestamp: 0,
            id,
            extended,
            rtr: false,
            dlc,
            data: payload,
            direction: CanDirection::Rx,
        }
    }
}
