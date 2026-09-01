//! `DataBuffer` 派生通道 — Math/Filter 等节点输出作为 Waveform sink 输入的缓冲
//!
//! 派生缓冲与主时间戳轴共享同一时间轴 (批首注册索引, 逐帧零哈希直写),
//! 派生键 (sink, source, source_handle) 随 DataBuffer 实例天然隔离。

use buffer_ring::RingBuffer;

use crate::DataBuffer;

/// 派生缓冲条目 (sink/widget 元数据 + 环形缓冲)
#[derive(Clone)]
// `derived` 模块私有, pub 与 pub(crate) 等效 (redundant_pub_crate 取 pub)
pub struct DerivedEntry {
    pub sink: String,
    pub source: String,
    pub source_handle: String,
    pub rb: RingBuffer<f32>,
}

impl DataBuffer {
    /// 推入派生数据 (与最近一次 push_frame 的时间戳对齐)
    ///
    /// 在数据平面中, 每帧图评估后调用:
    /// 遍历 graph.edges, 对每条 edge, 若 source 在输出快照中,
    /// 调用本方法将值 push 到完整 edge 源端身份对应的环形缓冲区。
    ///
    /// **时间对齐**: 派生缓冲区与 timestamps 共享同一时间轴,
    /// 保证 derived[i] 与 channels[ch][i] 对应同一帧。
    pub fn push_derived(&mut self, sink_id: &str, source_id: &str, value: f32) {
        self.push_derived_port(sink_id, source_id, "", value);
    }

    /// 推入带明确输出端口身份的派生数据。
    pub fn push_derived_port(
        &mut self,
        sink_id: &str,
        source_id: &str,
        source_handle: &str,
        value: f32,
    ) {
        let i = self.derived_port_index_of(sink_id, source_id, source_handle);
        self.push_derived_idx(i, value);
    }

    /// 派生缓冲索引: 命中返回下标; 未命中注册新条目 (环形缓冲容量 max_points)
    ///
    /// 供高通量路径在批首按 (sink_id, source_id, source_handle) 注册一次, 之后逐帧用
    /// [`push_derived_idx`] 零哈希直写。
    pub fn derived_index_of(&mut self, sink_id: &str, source_id: &str) -> usize {
        self.derived_port_index_of(sink_id, source_id, "")
    }

    /// 派生缓冲索引，以完整 edge 源端三元组区分同节点的不同输出口。
    pub fn derived_port_index_of(
        &mut self,
        sink_id: &str,
        source_id: &str,
        source_handle: &str,
    ) -> usize {
        let key = (
            sink_id.to_string(),
            source_id.to_string(),
            source_handle.to_string(),
        );
        if let Some(&idx) = self.derived_index.get(&key) {
            return idx;
        }
        let idx = self.derived_list.len();
        self.derived_list.push(DerivedEntry {
            sink: key.0.clone(),
            source: key.1.clone(),
            source_handle: key.2.clone(),
            rb: RingBuffer::new(self.max_points),
        });
        self.derived_index.insert(key, idx);
        idx
    }

    /// 按索引推入派生数据 (批内逐帧直写, 零哈希)
    ///
    /// 索引失效 (widget 删除导致调用方批内持有的下标越界) 时静默丢弃。
    pub fn push_derived_idx(&mut self, idx: usize, value: f32) {
        if let Some(e) = self.derived_list.get_mut(idx) {
            e.rb.push(value);
            self.version = self.version.wrapping_add(1);
        }
    }

    /// 读取派生通道最近 N 个点 (测试/显示消费端; 越界返回空)
    pub fn get_derived(&self, idx: usize, count: usize) -> Vec<f32> {
        self.derived_list
            .get(idx)
            .map_or(Vec::new(), |e| e.rb.recent(count))
    }

    /// 清空所有派生缓冲区 (断开连接/清数据时调用)
    pub fn clear_derived(&mut self) {
        self.derived_list.clear();
        self.derived_index.clear();
    }

    /// 移除指定 sink 的派生缓冲区 (widget 删除时调用)
    pub fn remove_derived_sink(&mut self, sink_id: &str) {
        self.derived_list.retain(|e| e.sink != sink_id);
        // retain 后下标移位, 重建索引映射
        self.derived_index = self
            .derived_list
            .iter()
            .enumerate()
            .map(|(i, e)| {
                (
                    (e.sink.clone(), e.source.clone(), e.source_handle.clone()),
                    i,
                )
            })
            .collect();
    }
}
