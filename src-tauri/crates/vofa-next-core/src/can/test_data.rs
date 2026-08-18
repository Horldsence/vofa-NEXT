//! CAN 帧测试数据生成器

use super::{CanDirection, CanFrame};

/// CAN 帧测试数据生成器
///
/// 提供各种模式生成 [`CanFrame`] 序列, 用于测试 CAN 缓冲区、负载统计和帧过滤。
///
/// # 示例
///
/// ```ignore
/// use vofa_next_core::{CanFrameTestData, CanBuffer};
///
/// let frames = CanFrameTestData::standard_frames(10);
/// let mut buf = CanBuffer::new(100);
/// for f in frames { buf.push(f); }
/// ```
pub struct CanFrameTestData;

#[allow(clippy::cast_possible_truncation)]
impl CanFrameTestData {
    /// 生成指定数量的标准帧, ID 从 `base_id` 开始递增
    ///
    /// 每帧携带 8 字节数据, 第一个字节等于帧序号 (0..count), 其余为 0。
    /// 时间戳从 0 开始, 每帧间隔 1000 微秒。
    pub fn standard_frames(base_id: u32, count: usize) -> Vec<CanFrame> {
        (0..count)
            .map(|i| {
                let mut data = vec![0u8; 8];
                data[0] = i as u8;
                CanFrame {
                    timestamp: i as u64 * 1000,
                    id: base_id + i as u32,
                    extended: false,
                    rtr: false,
                    dlc: 8,
                    data,
                    direction: CanDirection::Rx,
                }
            })
            .collect()
    }

    /// 生成指定数量的扩展帧, ID 从 `base_id` 开始递增
    pub fn extended_frames(base_id: u32, count: usize) -> Vec<CanFrame> {
        (0..count)
            .map(|i| {
                let mut data = vec![0u8; 8];
                data[0] = i as u8;
                CanFrame {
                    timestamp: i as u64 * 1000,
                    id: base_id + i as u32,
                    extended: true,
                    rtr: false,
                    dlc: 8,
                    data,
                    direction: CanDirection::Rx,
                }
            })
            .collect()
    }

    /// 生成具有相同 ID 和数据模式的重复帧
    ///
    /// 所有帧共享相同的 `id`、`data` 和 `extended` 标志,
    /// 时间戳从 0 开始每帧间隔 1000 微秒。
    pub fn repeating(id: u32, data: Vec<u8>, extended: bool, count: usize) -> Vec<CanFrame> {
        let dlc = data.len().min(8) as u8;
        (0..count)
            .map(|i| CanFrame {
                timestamp: i as u64 * 1000,
                id,
                extended,
                rtr: false,
                dlc,
                data: data[..dlc as usize].to_vec(),
                direction: CanDirection::Rx,
            })
            .collect()
    }

    /// 生成多 ID 循环帧
    ///
    /// 反复遍历 `ids` 列表生成帧, 每帧携带 `data_len` 字节数据。
    /// 时间戳从 0 开始每帧间隔 1000 微秒。
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

    /// 生成一帧用于负载测试 (带时间戳)
    ///
    /// 创建 `dlc` 字节的空数据帧, 适合推入 [`CanLoadStats`] 进行负载率计算测试。
    pub fn load_frame(id: u32, dlc: u8, timestamp_us: u64) -> CanFrame {
        CanFrame {
            timestamp: timestamp_us,
            id,
            extended: false,
            rtr: false,
            dlc: dlc.min(8),
            data: vec![0; dlc as usize],
            direction: CanDirection::Rx,
        }
    }

    /// 生成带指定数据模式的帧
    ///
    /// `data` 长度超过 8 时自动截断。
    pub fn with_data(id: u32, data: Vec<u8>, extended: bool) -> CanFrame {
        let dlc = data.len().min(8) as u8;
        CanFrame {
            timestamp: 0,
            id,
            extended,
            rtr: false,
            dlc,
            data: data[..dlc as usize].to_vec(),
            direction: CanDirection::Rx,
        }
    }
}
