//! DataBuffer 派生通道 — Math/Filter 等节点输出作为 Waveform sink 输入的缓冲
//!
//! 派生缓冲与主时间戳轴共享同一时间轴 (批首注册索引, 逐帧零哈希直写),
//! 派生键 (sink, source) 随 DataBuffer 实例天然隔离 (多源 = 每源一个实例)。

use crate::{DataBuffer, RingBuffer};

/// 派生缓冲条目 (sink/widget 元数据 + 环形缓冲)
pub(crate) struct DerivedEntry {
    pub(crate) sink: String,
    pub(crate) source: String,
    pub(crate) rb: RingBuffer<f32>,
}

impl DataBuffer {
    /// 推入派生数据 (与最近一次 push_frame 的时间戳对齐)
    ///
    /// 在数据平面中, 每帧图评估后调用:
    /// 遍历 graph.edges, 对每条 edge, 若 source 在输出快照中,
    /// 调用本方法将值 push 到 (sink_id, source_id) 的环形缓冲区。
    ///
    /// **时间对齐**: 派生缓冲区与 timestamps 共享同一时间轴,
    /// 保证 derived[i] 与 channels[ch][i] 对应同一帧。
    pub fn push_derived(&mut self, sink_id: &str, source_id: &str, value: f32) {
        let i = self.derived_index_of(sink_id, source_id);
        self.push_derived_idx(i, value);
    }

    /// 派生缓冲索引: 命中返回下标; 未命中注册新条目 (环形缓冲容量 max_points)
    ///
    /// 供高通量路径在批首按 (sink_id, source_id) 注册一次, 之后逐帧用
    /// [`push_derived_idx`] 零哈希直写。
    pub fn derived_index_of(&mut self, sink_id: &str, source_id: &str) -> usize {
        let key = (sink_id.to_string(), source_id.to_string());
        if let Some(&idx) = self.derived_index.get(&key) {
            return idx;
        }
        let idx = self.derived_list.len();
        self.derived_list.push(DerivedEntry {
            sink: key.0.clone(),
            source: key.1.clone(),
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
            .map(|(i, e)| ((e.sink.clone(), e.source.clone()), i))
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use crate::DataBuffer;
    use vofa_next_core::DataFrame;

    #[test]
    fn test_push_derived_aligned_with_timestamps() {
        // 场景: 3 帧, 每帧 push_frame 后 push_derived
        // 验证 derived 数据与 channels 时间戳对齐
        let mut buf = DataBuffer::new(100, 2);
        // 帧 0
        buf.push_frame(&DataFrame::new(vec![1.0, 2.0]));
        buf.push_derived("wave1", "math1", 10.0);
        // 帧 1
        buf.push_frame(&DataFrame::new(vec![3.0, 4.0]));
        buf.push_derived("wave1", "math1", 30.0);
        // 帧 2
        buf.push_frame(&DataFrame::new(vec![5.0, 6.0]));
        buf.push_derived("wave1", "math1", 50.0);

        let w = buf.get_recent(3);
        assert_eq!(w.channels[0], vec![1.0, 3.0, 5.0]);
        // derived 应与 channels 对齐
        let derived = w.derived.get("wave1").unwrap().get("math1").unwrap();
        assert_eq!(derived, &vec![10.0, 30.0, 50.0]);
    }

    #[test]
    fn test_derived_created_later_pads_nan() {
        // 场景: derived 缓冲区在第 2 帧才创建 (前 2 帧无 derived)
        let mut buf = DataBuffer::new(100, 1);
        buf.push_frame(&DataFrame::new(vec![1.0]));
        buf.push_frame(&DataFrame::new(vec![2.0]));
        // 第 3 帧才开始 push derived
        buf.push_frame(&DataFrame::new(vec![3.0]));
        buf.push_derived("wave1", "math1", 30.0);
        buf.push_frame(&DataFrame::new(vec![4.0]));
        buf.push_derived("wave1", "math1", 40.0);

        let w = buf.get_recent(4);
        assert_eq!(w.channels[0], vec![1.0, 2.0, 3.0, 4.0]);
        let derived = w.derived.get("wave1").unwrap().get("math1").unwrap();
        // 前 2 个应为 NaN, 后 2 个为实际值
        assert_eq!(derived.len(), 4);
        assert!(derived[0].is_nan());
        assert!(derived[1].is_nan());
        assert_eq!(derived[2], 30.0);
        assert_eq!(derived[3], 40.0);
    }

    #[test]
    fn test_multiple_derived_sources() {
        // 场景: 一个 sink 连接多个 source (math1, math2)
        let mut buf = DataBuffer::new(100, 1);
        buf.push_frame(&DataFrame::new(vec![1.0]));
        buf.push_derived("wave1", "math1", 10.0);
        buf.push_derived("wave1", "math2", 20.0);
        buf.push_frame(&DataFrame::new(vec![2.0]));
        buf.push_derived("wave1", "math1", 30.0);
        buf.push_derived("wave1", "math2", 40.0);

        let w = buf.get_recent(2);
        let sink_derived = w.derived.get("wave1").unwrap();
        assert_eq!(sink_derived.get("math1").unwrap(), &vec![10.0, 30.0]);
        assert_eq!(sink_derived.get("math2").unwrap(), &vec![20.0, 40.0]);
    }

    #[test]
    fn test_multiple_derived_sinks() {
        // 场景: 多个 sink 各自有 derived
        let mut buf = DataBuffer::new(100, 1);
        buf.push_frame(&DataFrame::new(vec![1.0]));
        buf.push_derived("wave1", "math1", 10.0);
        buf.push_derived("wave2", "math2", 20.0);

        let w = buf.get_recent(1);
        assert_eq!(
            w.derived.get("wave1").unwrap().get("math1").unwrap(),
            &vec![10.0]
        );
        assert_eq!(
            w.derived.get("wave2").unwrap().get("math2").unwrap(),
            &vec![20.0]
        );
    }

    #[test]
    fn test_clear_derived() {
        let mut buf = DataBuffer::new(100, 1);
        buf.push_frame(&DataFrame::new(vec![1.0]));
        buf.push_derived("wave1", "math1", 10.0);
        assert!(!buf.get_recent(1).derived.is_empty());

        buf.clear_derived();
        let w = buf.get_recent(1);
        assert!(w.derived.is_empty());
        // timestamps 和 channels 不受影响
        assert_eq!(w.channels[0], vec![1.0]);
    }

    #[test]
    fn test_remove_derived_sink() {
        let mut buf = DataBuffer::new(100, 1);
        buf.push_frame(&DataFrame::new(vec![1.0]));
        buf.push_derived("wave1", "math1", 10.0);
        buf.push_derived("wave2", "math2", 20.0);

        buf.remove_derived_sink("wave1");
        let w = buf.get_recent(1);
        assert!(!w.derived.contains_key("wave1"));
        assert!(w.derived.contains_key("wave2"));
    }

    #[test]
    fn test_derived_ringbuffer_overflow() {
        // 验证 derived 缓冲区也会覆盖旧数据
        let mut buf = DataBuffer::new(3, 1); // max_points = 3
        for i in 0..5 {
            buf.push_frame(&DataFrame::new(vec![i as f32]));
            buf.push_derived("wave1", "math1", (i * 10) as f32);
        }
        let w = buf.get_recent(3);
        // 只保留最近 3 个点
        assert_eq!(w.channels[0], vec![2.0, 3.0, 4.0]);
        let derived = w.derived.get("wave1").unwrap().get("math1").unwrap();
        assert_eq!(derived, &vec![20.0, 30.0, 40.0]);
    }

    #[test]
    fn test_derived_empty_buffer() {
        // 空 buffer 时 derived 也应为空
        let buf = DataBuffer::new(100, 2);
        let w = buf.get_recent(10);
        assert!(w.derived.is_empty());
    }
}
