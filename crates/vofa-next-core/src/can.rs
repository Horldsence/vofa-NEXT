use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

// ============ CAN 核心类型 ============

/// 单个 ID 的负载统计快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanIdLoadStats {
    pub id: u32,
    pub extended: bool,
    pub frame_count: u64,
    /// 总位数 (含位填充估算)
    pub total_bits: u64,
    /// 总字节数 (DLC 累加)
    pub total_bytes: u64,
}

/// CAN 负载统计快照 — 由滑动窗口计算得到
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanLoadSnapshot {
    /// 窗口大小 (微秒)
    pub window_us: u64,
    /// 窗口内总帧数
    pub frame_count: u64,
    /// 窗口内总位数 (含位填充估算)
    pub total_bits: u64,
    /// 窗口内总字节数
    pub total_bytes: u64,
    /// 当前负载率 (0.0 - 1.0+, 可超过 1.0 表示过载)
    pub load_ratio: f64,
    /// 时间序列采样 (最近的负载率历史, 用于绘制折线图)
    pub history: Vec<CanLoadHistoryPoint>,
    /// 按 ID 的负载分布 (按 total_bits 降序)
    pub per_id: Vec<CanIdLoadStats>,
    /// 按 ID 的负载率历史 (用于时序图叠加显示)
    /// key = "id-extended" 字符串 (便于 serde), value = 历史采样点
    pub per_id_history: Vec<CanIdLoadHistory>,
}

/// 单个 ID 的负载率历史 (用于时序图叠加)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanIdLoadHistory {
    pub id: u32,
    pub extended: bool,
    pub history: Vec<CanLoadHistoryPoint>,
}

/// 负载历史采样点
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CanLoadHistoryPoint {
    /// 时间戳 (微秒)
    pub timestamp: u64,
    /// 负载率 (0.0 - 1.0+)
    pub load_ratio: f64,
    /// 帧率 (帧/秒)
    pub fps: f64,
}

/// CAN 负载统计器 — 基于滑动时间窗
///
/// 每次推入一帧时, 自动剔除窗口外的旧样本, 并维护:
/// - 窗口内总位数 (用于负载率计算)
/// - 按 ID 的帧数 / 位数 / 字节数统计
/// - 最近 N 个采样点的负载率历史 (供前端绘制时序图)
///
/// 位数估算公式 (含 1.2 倍位填充因子):
/// - 标准帧: (47 + 8×DLC) × 1.2
/// - 扩展帧: (67 + 8×DLC) × 1.2
pub struct CanLoadStats {
    /// 滑动窗口内的帧样本 (timestamp_us, frame_bits, id, extended, dlc)
    samples: VecDeque<(u64, u32, u32, bool, u8)>,
    /// 窗口大小 (微秒)
    window_us: u64,
    /// 窗口内总位数 (累加, 避免每次扫描)
    total_bits: u64,
    /// 窗口内总字节数
    total_bytes: u64,
    /// 按 (id, extended) 的统计 (窗口内)
    per_id: HashMap<(u32, bool), CanIdLoadStats>,
    /// 负载率历史采样 (timestamp, load_ratio, fps)
    history: VecDeque<CanLoadHistoryPoint>,
    /// 按 ID 的负载率历史采样 (用于时序图叠加)
    per_id_history: HashMap<(u32, bool), VecDeque<CanLoadHistoryPoint>>,
    /// 历史采样最大保留数
    history_capacity: usize,
}

impl CanLoadStats {
    /// 创建负载统计器
    ///
    /// - `window_us`: 滑动窗口大小 (微秒), 例如 1_000_000 = 1 秒
    /// - `history_capacity`: 历史采样点最大保留数 (用于时序图)
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

    /// 设置滑动窗口大小 (微秒)
    pub fn set_window_us(&mut self, window_us: u64) {
        self.window_us = window_us.max(1);
        // 窗口缩小后, 主动剔除超期样本
        if let Some(&(ts, _, _, _, _)) = self.samples.back() {
            self.evict_expired(ts);
        }
    }

    /// 当前窗口大小 (微秒)
    pub const fn window_us(&self) -> u64 {
        self.window_us
    }

    /// 推入一帧, 更新窗口内统计
    pub fn push(&mut self, frame: &CanFrame) {
        let bits = Self::frame_bits(frame);
        // 先剔除过期样本 (以当前帧时间为基准)
        self.evict_expired(frame.timestamp);
        // 推入新样本
        self.samples
            .push_back((frame.timestamp, bits, frame.id, frame.extended, frame.dlc));
        self.total_bits += u64::from(bits);
        self.total_bytes += u64::from(frame.dlc);
        // 更新 per_id
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

    /// 采样当前负载率, 推入历史 (前端按固定间隔调用)
    /// 同时为每个当前窗口内的 ID 采样其独立负载率
    #[allow(clippy::cast_precision_loss)]
    pub fn sample_history(&mut self, bitrate: u32, now_us: u64) {
        // 先剔除过期样本
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
        // 容量裁剪
        while self.history.len() > self.history_capacity {
            self.history.pop_front();
        }

        // 为每个当前在窗口内的 ID 采样其独立负载率
        // per_id_load = entry.total_bits / window_bits
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
                fps: 0.0, // per-id 不单独算 fps
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

    /// 当前负载率 (0.0 - 1.0+, 可超过 1.0 表示过载)
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

    /// 当前帧率 (帧/秒)
    #[allow(clippy::cast_precision_loss)]
    pub fn fps(&self) -> f64 {
        if self.window_us == 0 {
            return 0.0;
        }
        (self.samples.len() as f64) * 1_000_000.0 / self.window_us as f64
    }

    /// 生成当前快照 (含历史采样 + per_id 排序 + per_id_history)
    pub fn snapshot(&self, bitrate: u32) -> CanLoadSnapshot {
        let mut per_id: Vec<CanIdLoadStats> = self.per_id.values().cloned().collect();
        per_id.sort_by_key(|b| std::cmp::Reverse(b.total_bits));
        // per_id_history 按 per_id 的 total_bits 降序输出 (与 per_id 一致)
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
            // 用 per_id 的 total_bits 排序 (找不到则排到最后)
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

    /// 清空所有统计
    pub fn clear(&mut self) {
        self.samples.clear();
        self.total_bits = 0;
        self.total_bytes = 0;
        self.per_id.clear();
        self.history.clear();
        self.per_id_history.clear();
    }

    /// 剔除窗口外的过期样本 (以 `now_us` 为基准)
    fn evict_expired(&mut self, now_us: u64) {
        let cutoff = now_us.saturating_sub(self.window_us);
        while let Some(&(ts, bits, id, ext, dlc)) = self.samples.front() {
            if ts < cutoff {
                self.samples.pop_front();
                self.total_bits = self.total_bits.saturating_sub(u64::from(bits));
                self.total_bytes = self.total_bytes.saturating_sub(u64::from(dlc));
                // 同步更新 per_id (若窗口内该 ID 已无样本, 则移除)
                if let Some(entry) = self.per_id.get_mut(&(id, ext)) {
                    entry.frame_count = entry.frame_count.saturating_sub(1);
                    entry.total_bits = entry.total_bits.saturating_sub(u64::from(bits));
                    entry.total_bytes = entry.total_bytes.saturating_sub(u64::from(dlc));
                    if entry.frame_count == 0 {
                        self.per_id.remove(&(id, ext));
                        // 同步移除该 ID 的历史采样 (避免遗留无用数据)
                        self.per_id_history.remove(&(id, ext));
                    }
                }
            } else {
                break;
            }
        }
    }

    /// CAN 帧位数估算 (含 1.2 倍位填充因子)
    ///
    /// 标准帧: SOF(1) + ID(11) + RTR(1) + IDE(1) + r0(1) + DLC(4) + Data(8×DLC) + CRC(15) + CRCdel(1) + ACK(1) + ACKdel(1) + EOF(7) + IFS(3) = 47 + 8×DLC
    /// 扩展帧: SOF(1) + ID-A(11) + SRR(1) + IDE(1) + ID-B(18) + RTR(1) + r1(1) + r0(1) + DLC(4) + Data(8×DLC) + CRC(15) + CRCdel(1) + ACK(1) + ACKdel(1) + EOF(7) + IFS(3) = 67 + 8×DLC
    pub fn frame_bits(frame: &CanFrame) -> u32 {
        let base = if frame.extended { 67 } else { 47 };
        let raw = base + 8 * u32::from(frame.dlc);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        {
            (f64::from(raw) * 1.2) as u32
        }
    }
}

// ============ CAN 核心类型 (原始定义继续) ============

/// CAN 帧方向
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CanDirection {
    Rx,
    Tx,
}

/// CAN 帧 — 标准化 CAN 数据模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanFrame {
    pub timestamp: u64,          // 微秒
    pub id: u32,                 // CAN ID (11 位标准 / 29 位扩展)
    pub extended: bool,          // true = 扩展帧
    pub rtr: bool,               // 远程帧
    pub dlc: u8,                 // 数据长度 (0-8)
    pub data: Vec<u8>,           // 数据 (最多 8 字节)
    pub direction: CanDirection, // 收/发方向
}

/// CAN 波特率预设
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CanBitrate {
    Bps100k,
    Bps125k,
    Bps250k,
    Bps500k,
    Bps1m,
}

impl CanBitrate {
    /// 返回波特率数值 (bps)
    pub const fn bps(&self) -> u32 {
        match self {
            Self::Bps100k => 100_000,
            Self::Bps125k => 125_000,
            Self::Bps250k => 250_000,
            Self::Bps500k => 500_000,
            Self::Bps1m => 1_000_000,
        }
    }

    /// slcan 波特率命令字符 (Lawicel 协议)
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

/// CAN 过滤器
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanFilter {
    pub id: u32,
    pub mask: u32,
    pub extended: bool,
}

/// CAN 批次 — 通过 Channel 推送到前端
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanFrameBatch {
    pub frames: Vec<CanFrame>,
}

/// candleLight USB 设备信息
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

/// CAN 帧环形缓冲区 — 用于前端订阅推送
///
/// 当达到 max_size 时, 新帧会挤掉最旧的帧 (FIFO 淘汰)。
pub struct CanBuffer {
    frames: VecDeque<CanFrame>,
    max_size: usize,
}

impl CanBuffer {
    pub fn new(max_size: usize) -> Self {
        Self {
            frames: VecDeque::with_capacity(max_size.min(8192)),
            max_size,
        }
    }

    /// 推入一帧 (超出容量时丢弃最旧帧)
    pub fn push(&mut self, frame: CanFrame) {
        if self.frames.len() >= self.max_size {
            self.frames.pop_front();
        }
        self.frames.push_back(frame);
    }

    /// 获取最近 count 帧 (按时间顺序返回, 旧的在前)
    pub fn get_recent(&self, count: usize) -> Vec<CanFrame> {
        let n = count.min(self.frames.len());
        self.frames.iter().rev().take(n).rev().cloned().collect()
    }

    /// 清空缓冲区
    pub fn clear(&mut self) {
        self.frames.clear();
    }

    /// 设置最大容量 (保留最近帧)
    pub fn set_max_size(&mut self, max_size: usize) {
        self.max_size = max_size.max(1);
        while self.frames.len() > self.max_size {
            self.frames.pop_front();
        }
    }

    /// 当前缓冲区中的帧数
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// 缓冲区是否为空
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}

// ============ CAN 帧测试数据生成器 ============

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

impl CanFrameTestData {
    /// 生成指定数量的标准帧, ID 从 `base_id` 开始递增
    ///
    /// 每帧携带 8 字节数据, 第一个字节等于帧序号 (0..count), 其余为 0。
    /// 时间戳从 0 开始, 每帧间隔 1000 微秒。
    #[allow(clippy::cast_possible_truncation)]
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
    #[allow(clippy::cast_possible_truncation)]
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
    #[allow(clippy::cast_possible_truncation)]
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
    #[allow(clippy::cast_possible_truncation)]
    pub fn cycling(ids: &[u32], data_len: u8, count: usize) -> Vec<CanFrame> {
        let dlc = data_len.min(8);
        (0..count)
            .map(|i| {
                let id = ids[i % ids.len()];
                let mut data = vec![0u8; dlc as usize];
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
    #[allow(clippy::cast_possible_truncation)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::cast_possible_truncation)]
    fn make_frame(id: u32, data: Vec<u8>) -> CanFrame {
        CanFrame {
            timestamp: 0,
            id,
            extended: false,
            rtr: false,
            dlc: data.len() as u8,
            data,
            direction: CanDirection::Rx,
        }
    }

    #[test]
    fn test_can_buffer_new_empty() {
        let buf = CanBuffer::new(10);
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
    }

    #[test]
    fn test_can_buffer_push_and_len() {
        let mut buf = CanBuffer::new(10);
        buf.push(make_frame(0x100, vec![0x01]));
        assert_eq!(buf.len(), 1);
        assert!(!buf.is_empty());
        buf.push(make_frame(0x200, vec![0x02]));
        assert_eq!(buf.len(), 2);
    }

    #[test]
    fn test_can_buffer_get_recent_basic() {
        let mut buf = CanBuffer::new(10);
        buf.push(make_frame(0x100, vec![0x01]));
        buf.push(make_frame(0x200, vec![0x02]));
        buf.push(make_frame(0x300, vec![0x03]));
        let recent = buf.get_recent(2);
        assert_eq!(recent.len(), 2);
        // 返回顺序: 旧 → 新
        assert_eq!(recent[0].id, 0x200);
        assert_eq!(recent[1].id, 0x300);
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn test_can_buffer_get_recent_returns_in_time_order() {
        let mut buf = CanBuffer::new(10);
        for i in 0..5u32 {
            buf.push(make_frame(0x100 + i, vec![i as u8]));
        }
        let recent = buf.get_recent(3);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].id, 0x102); // 最旧
        assert_eq!(recent[1].id, 0x103);
        assert_eq!(recent[2].id, 0x104); // 最新
    }

    #[test]
    fn test_can_buffer_get_recent_count_greater_than_len() {
        let mut buf = CanBuffer::new(10);
        buf.push(make_frame(0x100, vec![0x01]));
        buf.push(make_frame(0x200, vec![0x02]));
        // count > len 时应返回全部
        let recent = buf.get_recent(100);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].id, 0x100);
        assert_eq!(recent[1].id, 0x200);
    }

    #[test]
    fn test_can_buffer_get_recent_zero() {
        let mut buf = CanBuffer::new(10);
        buf.push(make_frame(0x100, vec![0x01]));
        let recent = buf.get_recent(0);
        assert_eq!(recent.len(), 0);
    }

    #[test]
    fn test_can_buffer_get_recent_empty_buffer() {
        let buf = CanBuffer::new(10);
        let recent = buf.get_recent(5);
        assert_eq!(recent.len(), 0);
    }

    #[test]
    fn test_can_buffer_clear() {
        let mut buf = CanBuffer::new(10);
        buf.push(make_frame(0x100, vec![0x01]));
        buf.push(make_frame(0x200, vec![0x02]));
        assert_eq!(buf.len(), 2);
        buf.clear();
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
    }

    #[test]
    fn test_can_buffer_clear_idempotent() {
        let mut buf = CanBuffer::new(10);
        buf.clear();
        buf.clear();
        assert!(buf.is_empty());
    }

    #[test]
    fn test_can_buffer_overflow_drops_oldest() {
        let mut buf = CanBuffer::new(3);
        buf.push(make_frame(0x100, vec![0x01]));
        buf.push(make_frame(0x200, vec![0x02]));
        buf.push(make_frame(0x300, vec![0x03]));
        // 第 4 帧应丢弃最旧的 0x100
        buf.push(make_frame(0x400, vec![0x04]));
        assert_eq!(buf.len(), 3);
        let all = buf.get_recent(3);
        assert_eq!(all[0].id, 0x200);
        assert_eq!(all[1].id, 0x300);
        assert_eq!(all[2].id, 0x400);
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn test_can_buffer_overflow_preserves_recent() {
        // 大量推入后, get_recent 应只返回最近 N 帧
        let mut buf = CanBuffer::new(5);
        for i in 0..10u32 {
            buf.push(make_frame(0x100 + i, vec![i as u8]));
        }
        assert_eq!(buf.len(), 5);
        let recent = buf.get_recent(3);
        assert_eq!(recent.len(), 3);
        // 最近 3 帧为 0x107, 0x108, 0x109
        assert_eq!(recent[0].id, 0x107);
        assert_eq!(recent[1].id, 0x108);
        assert_eq!(recent[2].id, 0x109);
    }

    #[test]
    fn test_can_buffer_max_size_one() {
        // 边界: max_size=1
        let mut buf = CanBuffer::new(1);
        buf.push(make_frame(0x100, vec![0x01]));
        assert_eq!(buf.len(), 1);
        buf.push(make_frame(0x200, vec![0x02]));
        assert_eq!(buf.len(), 1);
        let recent = buf.get_recent(1);
        assert_eq!(recent[0].id, 0x200);
    }

    #[test]
    fn test_can_buffer_preserves_frame_fields() {
        // 验证 push/get_recent 完整保留 CanFrame 字段
        let mut buf = CanBuffer::new(10);
        let original = CanFrame {
            timestamp: 12345,
            id: 0x7FF,
            extended: true,
            rtr: true,
            dlc: 8,
            data: vec![0xDE, 0xAD, 0xBE, 0xEF, 0x12, 0x34, 0x56, 0x78],
            direction: CanDirection::Tx,
        };
        buf.push(original.clone());
        let recent = buf.get_recent(1);
        assert_eq!(recent.len(), 1);
        let f = &recent[0];
        assert_eq!(f.timestamp, original.timestamp);
        assert_eq!(f.id, original.id);
        assert_eq!(f.extended, original.extended);
        assert_eq!(f.rtr, original.rtr);
        assert_eq!(f.dlc, original.dlc);
        assert_eq!(f.data, original.data);
        assert_eq!(f.direction, original.direction);
    }

    // ===== CanLoadStats tests =====

    fn make_load_frame(id: u32, dlc: u8, timestamp: u64) -> CanFrame {
        CanFrame {
            timestamp,
            id,
            extended: false,
            rtr: false,
            dlc,
            data: vec![0; dlc as usize],
            direction: CanDirection::Rx,
        }
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_load_stats_empty() {
        let stats = CanLoadStats::new(1_000_000, 60);
        let snap = stats.snapshot(500_000);
        assert_eq!(snap.frame_count, 0);
        assert_eq!(snap.total_bits, 0);
        assert_eq!(snap.load_ratio, 0.0);
        assert!(snap.per_id.is_empty());
    }

    #[test]
    fn test_load_stats_single_frame() {
        let mut stats = CanLoadStats::new(1_000_000, 60);
        // 标准帧 dlc=4: (47 + 32) × 1.2 = 94.8 → 94 bits
        let frame = make_load_frame(0x123, 4, 1_000_000);
        stats.push(&frame);
        let snap = stats.snapshot(500_000);
        assert_eq!(snap.frame_count, 1);
        assert_eq!(snap.total_bytes, 4);
        assert_eq!(snap.total_bits, 94);
        // 1s 窗口, 500kbps → 500_000 bits 窗口容量
        // load_ratio = 94 / 500_000 = 0.000188
        assert!((snap.load_ratio - 94.0 / 500_000.0).abs() < 1e-9);
        assert_eq!(snap.per_id.len(), 1);
        assert_eq!(snap.per_id[0].id, 0x123);
        assert_eq!(snap.per_id[0].frame_count, 1);
    }

    #[test]
    fn test_load_stats_extended_frame_more_bits() {
        let mut stats = CanLoadStats::new(1_000_000, 60);
        // 扩展帧 dlc=8: (67 + 64) × 1.2 = 157.2 → 157 bits
        let frame = CanFrame {
            timestamp: 1_000_000,
            id: 0x12345678,
            extended: true,
            rtr: false,
            dlc: 8,
            data: vec![0; 8],
            direction: CanDirection::Rx,
        };
        stats.push(&frame);
        let snap = stats.snapshot(500_000);
        assert_eq!(snap.total_bits, 157);
        assert!(snap.per_id[0].extended);
    }

    #[test]
    fn test_load_stats_window_eviction() {
        let mut stats = CanLoadStats::new(100_000, 60); // 100ms 窗口
                                                        // 推入 3 帧: t=100ms / t=200ms / t=300ms
        stats.push(&make_load_frame(0x100, 4, 100_000));
        stats.push(&make_load_frame(0x100, 4, 200_000));
        stats.push(&make_load_frame(0x100, 4, 300_000));
        // 在 t=300ms 时, 窗口 [200ms, 300ms] 内有 2 帧 (t=200ms 和 t=300ms)
        let snap = stats.snapshot(500_000);
        assert_eq!(snap.frame_count, 2);
        // t=100ms 已被剔除
    }

    #[test]
    fn test_load_stats_per_id_aggregation() {
        let mut stats = CanLoadStats::new(1_000_000, 60);
        stats.push(&make_load_frame(0x100, 4, 1_000_000));
        stats.push(&make_load_frame(0x100, 4, 1_100_000));
        stats.push(&make_load_frame(0x200, 8, 1_200_000));
        let snap = stats.snapshot(500_000);
        assert_eq!(snap.per_id.len(), 2);
        // 0x100 有 2 帧
        let id_100 = snap.per_id.iter().find(|s| s.id == 0x100).unwrap();
        assert_eq!(id_100.frame_count, 2);
        assert_eq!(id_100.total_bytes, 8);
        // 0x200 有 1 帧
        let id_200 = snap.per_id.iter().find(|s| s.id == 0x200).unwrap();
        assert_eq!(id_200.frame_count, 1);
        assert_eq!(id_200.total_bytes, 8);
    }

    #[test]
    fn test_load_stats_history_sampling() {
        let mut stats = CanLoadStats::new(1_000_000, 3);
        stats.sample_history(500_000, 1_000_000);
        stats.sample_history(500_000, 1_100_000);
        stats.sample_history(500_000, 1_200_000);
        stats.sample_history(500_000, 1_300_000);
        let snap = stats.snapshot(500_000);
        // 容量 3, 推入 4 次, 应保留最近 3 个
        assert_eq!(snap.history.len(), 3);
        assert_eq!(snap.history[0].timestamp, 1_100_000);
        assert_eq!(snap.history[2].timestamp, 1_300_000);
    }

    #[test]
    fn test_load_stats_clear() {
        let mut stats = CanLoadStats::new(1_000_000, 60);
        stats.push(&make_load_frame(0x100, 4, 1_000_000));
        stats.sample_history(500_000, 1_000_000);
        stats.clear();
        let snap = stats.snapshot(500_000);
        assert_eq!(snap.frame_count, 0);
        assert!(snap.history.is_empty());
        assert!(snap.per_id.is_empty());
    }

    #[test]
    fn test_load_stats_set_window_us() {
        let mut stats = CanLoadStats::new(1_000_000, 60);
        stats.push(&make_load_frame(0x100, 4, 1_000_000));
        stats.push(&make_load_frame(0x100, 4, 1_500_000));
        // 缩小窗口到 200ms, 在 t=1.5s 时窗口 [1.3s, 1.5s] 内只有 1 帧
        stats.set_window_us(200_000);
        let snap = stats.snapshot(500_000);
        assert_eq!(snap.frame_count, 1);
    }

    #[test]
    fn test_load_stats_frame_bits_formula() {
        // 标准帧 dlc=0: (47 + 0) × 1.2 = 56.4 → 56
        let f = make_load_frame(0x100, 0, 0);
        assert_eq!(CanLoadStats::frame_bits(&f), 56);
        // 标准帧 dlc=8: (47 + 64) × 1.2 = 133.2 → 133
        let f = make_load_frame(0x100, 8, 0);
        assert_eq!(CanLoadStats::frame_bits(&f), 133);
        // 扩展帧 dlc=0: (67 + 0) × 1.2 = 80.4 → 80
        let f = CanFrame {
            timestamp: 0,
            id: 0x12345678,
            extended: true,
            rtr: false,
            dlc: 0,
            data: vec![],
            direction: CanDirection::Rx,
        };
        assert_eq!(CanLoadStats::frame_bits(&f), 80);
        // 扩展帧 dlc=8: (67 + 64) × 1.2 = 157.2 → 157
        let f = CanFrame {
            timestamp: 0,
            id: 0x12345678,
            extended: true,
            rtr: false,
            dlc: 8,
            data: vec![0; 8],
            direction: CanDirection::Rx,
        };
        assert_eq!(CanLoadStats::frame_bits(&f), 157);
    }

    // ===== per_id_history 测试 =====

    #[test]
    fn test_load_stats_per_id_history_sampling() {
        let mut stats = CanLoadStats::new(1_000_000, 5);
        stats.push(&make_load_frame(0x100, 4, 1_000_000));
        stats.push(&make_load_frame(0x200, 4, 1_000_000));
        stats.sample_history(500_000, 1_000_000);
        stats.sample_history(500_000, 1_100_000);
        let snap = stats.snapshot(500_000);
        // 2 个 ID 都应该有历史
        assert_eq!(snap.per_id_history.len(), 2);
        // 每个 ID 各 2 个采样点
        for h in &snap.per_id_history {
            assert_eq!(h.history.len(), 2);
            assert!(h.id == 0x100 || h.id == 0x200);
        }
    }

    #[test]
    fn test_load_stats_per_id_history_capacity() {
        let mut stats = CanLoadStats::new(1_000_000, 3);
        stats.push(&make_load_frame(0x100, 4, 1_000_000));
        // 采样 5 次, 容量 3 → 保留最近 3 个
        for i in 0..5u64 {
            stats.sample_history(500_000, 1_000_000 + i * 100_000);
        }
        let snap = stats.snapshot(500_000);
        assert_eq!(snap.per_id_history.len(), 1);
        assert_eq!(snap.per_id_history[0].history.len(), 3);
        // 最早的 2 个被裁剪, 保留 t=1.2s, 1.3s, 1.4s
        assert_eq!(snap.per_id_history[0].history[0].timestamp, 1_200_000);
        assert_eq!(snap.per_id_history[0].history[2].timestamp, 1_400_000);
    }

    #[test]
    fn test_load_stats_per_id_history_clear() {
        let mut stats = CanLoadStats::new(1_000_000, 5);
        stats.push(&make_load_frame(0x100, 4, 1_000_000));
        stats.sample_history(500_000, 1_000_000);
        stats.clear();
        let snap = stats.snapshot(500_000);
        assert!(snap.per_id_history.is_empty());
    }

    #[test]
    fn test_load_stats_per_id_history_eviction() {
        // 当 ID 离开窗口时, 其历史也应被移除
        let mut stats = CanLoadStats::new(100_000, 5); // 100ms 窗口
        stats.push(&make_load_frame(0x100, 4, 100_000));
        stats.sample_history(500_000, 100_000);
        // 推入一帧 t=300ms, 使 t=100ms 的样本过期
        stats.push(&make_load_frame(0x200, 4, 300_000));
        stats.sample_history(500_000, 300_000);
        let snap = stats.snapshot(500_000);
        // 0x100 应已被剔除 (窗口 [200ms, 300ms] 不含 100ms)
        // 其 per_id_history 也应不存在
        assert!(snap.per_id.iter().all(|s| s.id != 0x100));
        assert!(snap.per_id_history.iter().all(|h| h.id != 0x100));
        // 0x200 保留
        assert!(snap.per_id_history.iter().any(|h| h.id == 0x200));
    }

    #[test]
    fn test_load_stats_per_id_history_load_ratio() {
        // 验证 per_id 的 load_ratio 计算
        let mut stats = CanLoadStats::new(1_000_000, 5);
        // 标准帧 dlc=4: 94 bits
        stats.push(&make_load_frame(0x100, 4, 1_000_000));
        stats.sample_history(500_000, 1_000_000);
        let snap = stats.snapshot(500_000);
        let h = &snap.per_id_history[0];
        // window_bits = 1s × 500_000 bps = 500_000 bits
        // id_load = 94 / 500_000 = 0.000188
        assert!((h.history[0].load_ratio - 94.0 / 500_000.0).abs() < 1e-9);
    }
}
