//! `DataBuffer` 派生通道 — Math/Filter 等节点输出作为 Waveform sink 输入的缓冲
//!
//! 记录/求值解耦 (数据平面不变量 3): 派生值由**求值平面**写入, 携带显式时间戳
//! 并独立成轴 — 求值积压/丢批只表现为派生序列的时间缺口 (查询按时间对齐补
//! NaN), 绝不拖累或错位原始通道的时间轴。派生键 (sink, source, source_handle)
//! 随 DataBuffer 实例天然隔离。
//!
//! `DerivedStore: Clone` 为深拷贝 — `DataBuffer::clone` 借此实现"克隆即冻结"。

use buffer_ring::RingBuffer;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

/// 派生缓冲条目 (sink/widget 元数据 + 独立时间轴环形缓冲)
#[derive(Clone)]
// `derived` 模块私有, pub 与 pub(crate) 等效 (redundant_pub_crate 取 pub)
pub struct DerivedEntry {
    pub sink: String,
    pub source: String,
    pub source_handle: String,
    /// 派生采样时间戳 (µs, 与写入值一一对应)
    pub timestamps: RingBuffer<u64>,
    pub rb: RingBuffer<f32>,
}

/// 派生通道存储 — 求值平面专属, 与原始通道仅共享容量参数
#[derive(Clone, Default)]
pub struct DerivedStore {
    entries: Vec<DerivedEntry>,
    index: HashMap<(String, String, String), usize>,
    /// 版本号 (锁内维护; DataBuffer 缓存用于免锁变化检测)
    version: u64,
}

impl DerivedStore {
    /// 派生缓冲索引: 命中返回下标; 未命中注册新条目 (时间戳/值双环, 容量 max_points)
    pub fn derived_port_index_of(
        &mut self,
        sink_id: &str,
        source_id: &str,
        source_handle: &str,
        max_points: usize,
    ) -> usize {
        let key = (
            sink_id.to_string(),
            source_id.to_string(),
            source_handle.to_string(),
        );
        if let Some(&idx) = self.index.get(&key) {
            return idx;
        }
        let idx = self.entries.len();
        self.entries.push(DerivedEntry {
            sink: key.0.clone(),
            source: key.1.clone(),
            source_handle: key.2.clone(),
            timestamps: RingBuffer::new(max_points),
            rb: RingBuffer::new(max_points),
        });
        self.index.insert(key, idx);
        idx
    }

    /// 按索引推入带显式时间戳的派生数据 (求值平面逐帧调用, 零哈希)
    ///
    /// 索引失效 (widget 删除导致调用方批内持有的下标越界) 时静默丢弃。
    pub fn push_derived_ts_idx(&mut self, idx: usize, timestamp: u64, value: f32) {
        if let Some(e) = self.entries.get_mut(idx) {
            e.timestamps.push(timestamp);
            e.rb.push(value);
            self.version = self.version.wrapping_add(1);
        }
    }

    /// 按升序时间戳序列对齐取派生值 (合并线性走查, O(n+m));
    /// 未命中的位置 (求值落后/丢批缺口) 返回 NaN — 显示为断线而非错位
    pub fn values_at_timestamps(&self, idx: usize, timestamps: &[u64]) -> Vec<f32> {
        let Some(entry) = self.entries.get(idx) else {
            return vec![f32::NAN; timestamps.len()];
        };
        let mut out = Vec::with_capacity(timestamps.len());
        let mut ptr = 0_usize;
        let len = entry.timestamps.len();
        for &ts in timestamps {
            while ptr < len && entry.timestamps.get(ptr).is_some_and(|t| *t < ts) {
                ptr += 1;
            }
            if ptr < len && entry.timestamps.get(ptr) == Some(&ts) {
                out.push(entry.rb.get(ptr).copied().unwrap_or(f32::NAN));
                ptr += 1;
            } else {
                out.push(f32::NAN);
            }
        }
        out
    }

    pub fn entry(&self, idx: usize) -> Option<&DerivedEntry> {
        self.entries.get(idx)
    }

    pub fn entries(&self) -> &[DerivedEntry] {
        &self.entries
    }

    pub const fn version(&self) -> u64 {
        self.version
    }

    /// 派生序列主要内存估算 (时间戳 + 值双环)
    pub const fn estimated_bytes(&self, max_points: usize) -> usize {
        self.entries
            .len()
            .saturating_mul(max_points.saturating_mul(12))
    }

    /// 容量调整 (保留最近数据)
    pub fn resize(&mut self, max_points: usize) {
        for e in &mut self.entries {
            e.timestamps.resize(max_points);
            e.rb.resize(max_points);
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.index.clear();
        self.version = self.version.wrapping_add(1);
    }

    /// 移除指定 sink 的派生缓冲区 (widget 删除时调用)
    pub fn remove_sink(&mut self, sink_id: &str) {
        self.entries.retain(|e| e.sink != sink_id);
        // retain 后下标移位, 重建索引映射
        self.index = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| {
                (
                    (e.sink.clone(), e.source.clone(), e.source_handle.clone()),
                    i,
                )
            })
            .collect();
        self.version = self.version.wrapping_add(1);
    }
}

/// 派生存储句柄 — DataBuffer 内部持有; 记录平面 (原始通道) 与求值平面
/// (派生通道) 各自只锁自己的 Mutex, 互不阻塞
pub type SharedDerivedStore = Arc<Mutex<DerivedStore>>;

pub fn shared_derived_store() -> SharedDerivedStore {
    Arc::new(Mutex::new(DerivedStore::default()))
}

use crate::DataBuffer;

impl DataBuffer {
    /// 读取派生通道最近 N 个值 (测试/显示消费端; 越界返回空)
    pub fn get_derived(&self, idx: usize, count: usize) -> Vec<f32> {
        self.derived
            .lock()
            .entry(idx)
            .map_or(Vec::new(), |e| e.rb.recent(count))
    }

    /// 无显式输出端口身份的派生索引 (单输出节点)
    pub fn derived_index_of(&self, sink_id: &str, source_id: &str) -> usize {
        self.derived_port_index_of(sink_id, source_id, "")
    }

    /// 派生缓冲索引: 批首按 (sink_id, source_id, source_handle) 注册一次,
    /// 之后批内用 [`DataBuffer::push_derived_ts_idx`] 零哈希直写
    pub fn derived_port_index_of(
        &self,
        sink_id: &str,
        source_id: &str,
        source_handle: &str,
    ) -> usize {
        self.derived.lock().derived_port_index_of(
            sink_id,
            source_id,
            source_handle,
            self.max_points,
        )
    }

    /// 按索引推入带显式时间戳的派生数据 (求值平面; 与原始通道锁分离)
    pub fn push_derived_ts_idx(&self, idx: usize, timestamp: u64, value: f32) {
        let mut store = self.derived.lock();
        store.push_derived_ts_idx(idx, timestamp, value);
        self.derived_version
            .store(store.version(), Ordering::Relaxed);
    }

    /// 移除指定 sink 的派生缓冲区 (widget 删除时调用)
    pub fn remove_derived_sink(&self, sink_id: &str) {
        let mut store = self.derived.lock();
        store.remove_sink(sink_id);
        self.derived_version
            .store(store.version(), Ordering::Relaxed);
    }

    /// 清空所有派生缓冲区 (断开连接/清数据时调用)
    pub fn clear_derived(&self) {
        let mut store = self.derived.lock();
        store.clear();
        self.derived_version
            .store(store.version(), Ordering::Relaxed);
    }
}
