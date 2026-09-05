//! VNDP/VENV 二进制编码 — 线上契约帧 (小端列式样本 + 包络)

use data_bus::{SampleBatch, SampleStatus};

pub const BINARY_SCHEMA_VERSION: u16 = 1;
const SAMPLE_EVENT_KIND: u16 = 1;
pub(super) const SAMPLE_HEADER_LEN: usize = 68;

const fn status_code(status: &SampleStatus) -> u16 {
    match status {
        SampleStatus::Waiting => 0,
        SampleStatus::Live => 1,
        SampleStatus::Disconnected => 2,
        SampleStatus::ChannelOutOfRange { .. } => 3,
        SampleStatus::Overrun { .. } => 4,
    }
}

/// VNDP v1 little-endian columnar sample envelope.
pub(super) fn encode_samples(batch: &SampleBatch) -> Vec<u8> {
    let count = batch.samples.len();
    let validity_len = count.saturating_add(7) / 8;
    let payload_len = count.saturating_mul(16).saturating_add(validity_len);
    let mut bytes = Vec::with_capacity(SAMPLE_HEADER_LEN + payload_len);
    bytes.extend_from_slice(b"VNDP");
    bytes.extend_from_slice(&BINARY_SCHEMA_VERSION.to_le_bytes());
    bytes.extend_from_slice(&SAMPLE_EVENT_KIND.to_le_bytes());
    bytes.extend_from_slice(&status_code(&batch.status).to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&batch.sequence.to_le_bytes());
    bytes.extend_from_slice(
        &batch
            .samples
            .first()
            .map_or(0, |sample| sample.sequence)
            .to_le_bytes(),
    );
    bytes.extend_from_slice(&u32::try_from(count).unwrap_or(u32::MAX).to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&batch.preview_skipped.to_le_bytes());
    bytes.extend_from_slice(&batch.retention_evicted.to_le_bytes());
    bytes.extend_from_slice(&batch.ingress_dropped.to_le_bytes());
    bytes.extend_from_slice(&u32::try_from(payload_len).unwrap_or(u32::MAX).to_le_bytes());
    bytes.extend_from_slice(&u32::try_from(SAMPLE_HEADER_LEN).unwrap().to_le_bytes());
    for sample in batch.samples.iter() {
        bytes.extend_from_slice(&sample.timestamp_us.to_le_bytes());
    }
    for sample in batch.samples.iter() {
        bytes.extend_from_slice(&sample.value.to_le_bytes());
    }
    for byte_index in 0..validity_len {
        let remaining = count.saturating_sub(byte_index * 8);
        let valid_bits = remaining.min(8);
        bytes.push(if valid_bits == 8 {
            u8::MAX
        } else {
            (1_u8 << valid_bits) - 1
        });
    }
    bytes
}

/// 包络事件 kind (VNDP 体系内独立编号, 前端按 magic+kind 分派)
const ENVELOPE_EVENT_KIND: u16 = 2;
const ENVELOPE_HEADER_LEN: usize = 60;
/// 单次包络压缩的窗口点数上限 (1M 点 CPU 参考 ~30ms / GPU ~10ms, 33ms 节拍内)
pub(super) const ENVELOPE_WINDOW_CAP: usize = 1_000_000;

/// VENV v1 little-endian 包络帧: 头 + 每通道 columns×(f32 min, f32 max, u32 count)
///
/// 空列 (无有效样本): min=+inf / max=-inf / count=0 — 前端按断线处理。
#[allow(clippy::cast_possible_truncation)] // columns 已 clamp 16..=4096; 计数字段超 u32 视为饱和
pub(super) fn encode_envelope(
    seq: u64,
    window: &buffer_databuffer::WaveformWindow,
    columns: usize,
    envelopes: &[gpu_core::Envelope],
) -> Vec<u8> {
    let channel_count = envelopes.len();
    let per_channel = columns * 12;
    let payload_len = channel_count * per_channel;
    let mut bytes = Vec::with_capacity(ENVELOPE_HEADER_LEN + payload_len);
    bytes.extend_from_slice(b"VENV");
    bytes.extend_from_slice(&BINARY_SCHEMA_VERSION.to_le_bytes());
    bytes.extend_from_slice(&ENVELOPE_EVENT_KIND.to_le_bytes());
    bytes.extend_from_slice(&seq.to_le_bytes());
    bytes.extend_from_slice(
        &u32::try_from(window.timestamps.len())
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    bytes.extend_from_slice(&(u32::try_from(columns).unwrap_or(u32::MAX)).to_le_bytes());
    bytes.extend_from_slice(&(u32::try_from(channel_count).unwrap_or(u32::MAX)).to_le_bytes());
    // envelope 线上协议的窗口边界为 i64 毫秒 (前端 BigInt64 解码);
    // WaveformWindow.timestamps 已是 f64 毫秒, 打包时四舍五入取整
    let first_ts_ms = window.timestamps.first().copied().unwrap_or(0.0).round();
    let last_ts_ms = window.timestamps.last().copied().unwrap_or(0.0).round();
    #[allow(clippy::cast_possible_truncation)]
    {
        bytes.extend_from_slice(&(first_ts_ms as i64).to_le_bytes());
        bytes.extend_from_slice(&(last_ts_ms as i64).to_le_bytes());
    }
    bytes.extend_from_slice(
        &(u32::try_from(window.buffer_points).unwrap_or(u32::MAX)).to_le_bytes(),
    );
    bytes.extend_from_slice(
        &(u32::try_from(window.buffer_capacity).unwrap_or(u32::MAX)).to_le_bytes(),
    );
    bytes.extend_from_slice(&u32::try_from(payload_len).unwrap_or(u32::MAX).to_le_bytes());
    bytes.extend_from_slice(
        &u32::try_from(ENVELOPE_HEADER_LEN)
            .unwrap_or(60)
            .to_le_bytes(),
    );
    for env in envelopes {
        for v in &env.min {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        for v in &env.max {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        for c in &env.count {
            bytes.extend_from_slice(&c.to_le_bytes());
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use data_bus::{Sample, TopicKey};
    use std::sync::Arc;

    #[test]
    fn binary_sample_contract_has_stable_header_and_zero_value() {
        let batch = SampleBatch {
            topic: TopicKey::new("FireWater", "ch3"),
            sequence: 7,
            samples: Arc::from([Sample {
                sequence: 9,
                timestamp_us: 11,
                value: 0.0,
            }]),
            status: SampleStatus::Live,
            preview_skipped: 1,
            retention_evicted: 2,
            ingress_dropped: 3,
        };
        let bytes = encode_samples(&batch);
        assert_eq!(&bytes[..4], b"VNDP");
        assert_eq!(bytes.len(), SAMPLE_HEADER_LEN + 17);
        assert!(f64::from_le_bytes(bytes[76..84].try_into().unwrap()).abs() < f64::EPSILON);
        assert_eq!(bytes[84], 1);
    }
}
