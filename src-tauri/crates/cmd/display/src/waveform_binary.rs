//! 波形窗口 WWB1 二进制列编码 — 波形 detail/overview 流的唯一 IPC 载荷
//!
//! 30fps × 每系列 12k 点的 JSON 数字数组在序列化/解析两侧都是全量拷贝
//! (且 serde_json 把 NaN 编码为 null, 前端还需一次 null→NaN 归一化);
//! 列式二进制让前端以 TypedArray 视图零拷贝读取, NaN 原生传递。
//!
//! 布局 (little-endian):
//! ```text
//! [0..4)    magic "WWB1"
//! [4..6)    schema 版本 = 1 (u16)
//! [6..8)    sampling (u16: 0=raw 1=min_max 2=lttb)
//! [8..16)   seq (u64)
//! [16..24)  latest_timestamp_us (u64)
//! [24..32)  buffer_points (u64)
//! [32..40)  buffer_capacity (u64)
//! [40..48)  raw_window_points (u64)
//! [48..52)  point_count n (u32)
//! [52..56)  channel_slot_count c (u32)
//! [56..60)  channel_count (u32)
//! [60..64)  derived_entry_count d (u32)
//! [64..)    timestamps: n × f64 (相对最新的毫秒偏移)
//! [..)      channels: c 列 × n × f32 (槽位全形, 空槽为 NaN)
//! [..)      derived × d: (u16 长度 + UTF-8 bytes) × {sink, source, handle}
//!                      + n × f32
//! ```

use buffer_databuffer::{WaveformSampling, WaveformWindow};

pub const WAVEFORM_BINARY_MAGIC: &[u8; 4] = b"WWB1";
pub const WAVEFORM_BINARY_SCHEMA_VERSION: u16 = 1;
pub const WAVEFORM_BINARY_HEADER_LEN: usize = 64;

const fn sampling_code(sampling: WaveformSampling) -> u16 {
    match sampling {
        WaveformSampling::Raw => 0,
        WaveformSampling::MinMax => 1,
        WaveformSampling::Lttb => 2,
    }
}

/// 把通道列对齐到 n 点 (截断补齐) — 快照列与时间戳本就等长, 此为防御性对齐
fn aligned_column(column: &[f32], n: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(n);
    out.extend_from_slice(&column[..column.len().min(n)]);
    out.resize(n, f32::NAN);
    out
}

pub fn encode_waveform_window(window: &WaveformWindow) -> Vec<u8> {
    let n = window.timestamps.len();
    let slots = window.channels.len();
    let derived_entries: Vec<(&String, &String, &String, &Vec<f32>)> = window
        .derived
        .iter()
        .flat_map(|(sink, sources)| {
            sources.iter().flat_map(move |(source, handles)| {
                handles
                    .iter()
                    .map(move |(handle, values)| (sink, source, handle, values))
            })
        })
        .collect();

    // 每条派生项: 3×(u16 长度 + 键名字节) + 补齐到 4 字节 + n × f32
    // (补齐保证 f32 列 4 字节对齐 — 前端 Float32Array 视图不允许错位偏移)
    let payload_len = 8 * n
        + 4 * slots * n
        + derived_entries
            .iter()
            .map(|(sink, source, handle, _)| {
                let keys_len = 6 + sink.len() + source.len() + handle.len();
                keys_len.next_multiple_of(4) + 4 * n
            })
            .sum::<usize>();
    let mut bytes = Vec::with_capacity(WAVEFORM_BINARY_HEADER_LEN + payload_len);
    bytes.extend_from_slice(WAVEFORM_BINARY_MAGIC);
    bytes.extend_from_slice(&WAVEFORM_BINARY_SCHEMA_VERSION.to_le_bytes());
    bytes.extend_from_slice(&sampling_code(window.sampling).to_le_bytes());
    bytes.extend_from_slice(&window.seq.to_le_bytes());
    bytes.extend_from_slice(&window.latest_timestamp_us.to_le_bytes());
    bytes.extend_from_slice(
        &u64::try_from(window.buffer_points)
            .unwrap_or(u64::MAX)
            .to_le_bytes(),
    );
    bytes.extend_from_slice(
        &u64::try_from(window.buffer_capacity)
            .unwrap_or(u64::MAX)
            .to_le_bytes(),
    );
    bytes.extend_from_slice(
        &u64::try_from(window.raw_window_points)
            .unwrap_or(u64::MAX)
            .to_le_bytes(),
    );
    bytes.extend_from_slice(&u32::try_from(n).unwrap_or(u32::MAX).to_le_bytes());
    bytes.extend_from_slice(&u32::try_from(slots).unwrap_or(u32::MAX).to_le_bytes());
    bytes.extend_from_slice(
        &u32::try_from(window.channel_count)
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    bytes.extend_from_slice(
        &u32::try_from(derived_entries.len())
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );

    for ts in &window.timestamps {
        bytes.extend_from_slice(&ts.to_le_bytes());
    }
    for column in &window.channels {
        for value in aligned_column(column, n) {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    for (sink, source, handle, values) in derived_entries {
        for key in [sink, source, handle] {
            let len = u16::try_from(key.len()).unwrap_or(u16::MAX);
            bytes.extend_from_slice(&len.to_le_bytes());
            bytes.extend_from_slice(key.as_bytes());
        }
        while bytes.len() % 4 != 0 {
            bytes.push(0);
        }
        for value in aligned_column(values, n) {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn sample_window() -> WaveformWindow {
        let mut derived = HashMap::new();
        let mut sources = HashMap::new();
        let mut handles = HashMap::new();
        handles.insert("out".to_string(), vec![1.0, f32::NAN, 3.0]);
        sources.insert("math-1".to_string(), handles);
        derived.insert("wave-1".to_string(), sources);
        WaveformWindow {
            seq: 42,
            timestamps: vec![-30.0, -20.0, -10.0],
            channels: vec![vec![10.0, 11.0, 12.0], vec![], vec![30.0, 31.0, 32.0]],
            channel_count: 3,
            derived,
            buffer_points: 1_000,
            buffer_capacity: 100_000,
            latest_timestamp_us: 1_234_567,
            raw_window_points: 2_000,
            sampling: WaveformSampling::MinMax,
        }
    }

    #[test]
    fn header_and_columns_are_encoded() {
        let bytes = encode_waveform_window(&sample_window());
        // 键名 15 字节 + 补齐 3 = 每项键区对齐到 24; (6+键名+补齐+4n) = 36
        let expected = 64 + 8 * 3 + 4 * 3 * 3 + 24 + 4 * 3;
        assert_eq!(bytes.len(), expected);
        let view = &bytes[..WAVEFORM_BINARY_HEADER_LEN];
        assert_eq!(u16::from_le_bytes(view[4..6].try_into().unwrap()), 1);
        assert_eq!(u16::from_le_bytes(view[6..8].try_into().unwrap()), 1); // min_max
        assert_eq!(u64::from_le_bytes(view[8..16].try_into().unwrap()), 42);
        assert_eq!(u32::from_le_bytes(view[48..52].try_into().unwrap()), 3);
        assert_eq!(u32::from_le_bytes(view[52..56].try_into().unwrap()), 3);
        assert_eq!(u32::from_le_bytes(view[56..60].try_into().unwrap()), 3);
        assert_eq!(u32::from_le_bytes(view[60..64].try_into().unwrap()), 1);
        // timestamps
        let ts: f64 = f64::from_le_bytes(bytes[64..72].try_into().unwrap());
        assert!((ts + 30.0).abs() < 1e-9);
        // 通道 1 空槽对齐为 NaN
        let ch1_offset = 64 + 8 * 3 + 4 * 3;
        let v: f32 = f32::from_le_bytes(bytes[ch1_offset..ch1_offset + 4].try_into().unwrap());
        assert!(v.is_nan());
        // 派生列 NaN 原生传递 (JSON 路径下这里曾是 null); 值列 4 字节对齐
        let derived_offset = 64 + 8 * 3 + 4 * 3 * 3 + 24;
        assert_eq!(derived_offset % 4, 0);
        let v: f32 = f32::from_le_bytes(
            bytes[derived_offset + 4..derived_offset + 8]
                .try_into()
                .unwrap(),
        );
        assert!(v.is_nan());
    }

    #[test]
    fn short_columns_are_padded_to_point_count() {
        let mut window = sample_window();
        window.channels = vec![vec![1.0]]; // 长度不足
        let bytes = encode_waveform_window(&window);
        // n 仍由 timestamps 决定 = 3; 键区 21 对齐到 24
        let expected = 64 + 8 * 3 + 4 * 3 + 24 + 4 * 3;
        assert_eq!(bytes.len(), expected);
    }
}
