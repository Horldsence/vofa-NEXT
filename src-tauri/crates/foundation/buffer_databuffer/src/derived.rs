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
    /// 结构变更世代，防止旧求值批次的索引指向清空后新建的序列。
    generation: u64,
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

    /// 按升序时间戳序列对齐取派生值；连续点常数推进，跨历史缺口指数查找。
    /// 未命中的位置 (求值落后/丢批缺口) 返回 NaN — 显示为断线而非错位
    pub fn values_at_timestamps(&self, idx: usize, timestamps: &[u64]) -> Vec<f32> {
        let Some(entry) = self.entries.get(idx) else {
            return vec![f32::NAN; timestamps.len()];
        };
        let mut out = Vec::with_capacity(timestamps.len());
        let mut ptr = 0_usize;
        let len = entry.timestamps.len();
        for &ts in timestamps {
            if ptr < len && entry.timestamps.get(ptr).is_some_and(|t| *t < ts) {
                // 先指数扩展再二分；最近小窗口不扫描整个历史，密集窗口也不
                // 为每个相邻点付出 log(容量) 次比较。逻辑索引兼容环形回绕。
                let mut step = 1_usize;
                let mut hi = ptr.saturating_add(step).min(len);
                while hi < len && entry.timestamps.get(hi).is_some_and(|t| *t < ts) {
                    ptr = hi + 1;
                    step = step.saturating_mul(2);
                    hi = ptr.saturating_add(step).min(len);
                }
                while ptr < hi {
                    let mid = ptr + (hi - ptr) / 2;
                    if entry.timestamps.get(mid).is_some_and(|t| *t < ts) {
                        ptr = mid + 1;
                    } else {
                        hi = mid;
                    }
                }
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
        self.generation = self.generation.wrapping_add(1);
        for e in &mut self.entries {
            e.timestamps.resize(max_points);
            e.rb.resize(max_points);
        }
    }

    pub fn clear(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.entries.clear();
        self.index.clear();
        self.version = self.version.wrapping_add(1);
    }

    /// 移除指定 sink 的派生缓冲区 (widget 删除时调用)
    pub fn remove_sink(&mut self, sink_id: &str) {
        self.generation = self.generation.wrapping_add(1);
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

/// 求值平面持有的轻量派生写句柄。
///
/// 句柄只共享派生存储与版本号，不持有 [`DataBuffer`] 的原始数据锁。评估 worker
/// 因此可以在整批图求值期间写派生序列，而不会阻塞记录平面追加原始波形。
#[derive(Clone)]
pub struct DerivedWriter {
    store: SharedDerivedStore,
    version: Arc<std::sync::atomic::AtomicU64>,
    max_points: usize,
    generation: u64,
}

impl DerivedWriter {
    /// 注册或查找一个派生序列。
    pub fn port_index_of(&self, sink_id: &str, source_id: &str, source_handle: &str) -> usize {
        let mut store = self.store.lock();
        if store.generation != self.generation {
            return usize::MAX;
        }
        store.derived_port_index_of(sink_id, source_id, source_handle, self.max_points)
    }

    /// 批量追加派生样本，只获取一次派生存储锁。
    ///
    /// 输入三元组为 `(派生索引, 时间戳, 值)`；保持迭代顺序写入，使同一序列
    /// 的时间轴语义与逐点写入完全一致。
    pub fn append<I>(&self, samples: I)
    where
        I: IntoIterator<Item = (usize, u64, f32)>,
    {
        let mut samples = samples.into_iter().peekable();
        if samples.peek().is_none() {
            return;
        }
        let mut store = self.store.lock();
        if store.generation != self.generation {
            return;
        }
        for (index, timestamp, value) in samples {
            store.push_derived_ts_idx(index, timestamp, value);
        }
        self.version.store(store.version(), Ordering::Relaxed);
    }

    /// 追加一个派生样本（低频命令/测试兼容入口）。
    pub fn push(&self, index: usize, timestamp: u64, value: f32) {
        self.append(std::iter::once((index, timestamp, value)));
    }
}

use crate::DataBuffer;

impl DataBuffer {
    /// 获取与原始数据外层锁解耦的派生写句柄。
    pub fn derived_writer(&self) -> DerivedWriter {
        DerivedWriter {
            store: Arc::clone(&self.derived),
            version: Arc::clone(&self.derived_version),
            max_points: self.max_points,
            generation: self.derived.lock().generation,
        }
    }

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
        self.derived_writer()
            .port_index_of(sink_id, source_id, source_handle)
    }

    /// 按索引推入带显式时间戳的派生数据 (求值平面; 与原始通道锁分离)
    pub fn push_derived_ts_idx(&self, idx: usize, timestamp: u64, value: f32) {
        self.derived_writer().push(idx, timestamp, value);
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
