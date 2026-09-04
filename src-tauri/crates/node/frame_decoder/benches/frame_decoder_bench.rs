//! 帧解码状态机基准 — `FrameParser::feed` 字节吞吐
//!
//! 场景: 典型块组合 (帧头 AA + N×UInt16LE 字段 [+ Sum8 校验] + 帧尾 BB),
//! 2048 帧连续字节流单次喂入; 度量字节吞吐与逐帧解析成本。
//! - `u16le_4ch`: 4 字段小帧 (10 B/帧)
//! - `u16le_4ch_sum8`: 同帧 + Inline Sum8 校验 (校验计算的增量成本)
//! - `u16le_16ch`: 16 字段宽帧 (34 B/帧, 字段线性扫描的宽度扩展)
//!
//! 说明: 喂入为整帧对齐字节流 (解析后缓冲清空, 迭代间状态无残留);
//! eval 侧 FrameDecoder op 的槽位写入由 eval eval_run_bench / graph_eval 覆盖,
//! DecoderBlock 组合的语义闭环由 roundtrip_tests 仲裁。

use std::collections::HashMap;
use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use frame_decoder::{ChecksumAlgorithm, FrameDecoderTestData, FrameParser};
use schema_types::{DecoderBlockDef, DecoderChecksumCover, DecoderChecksumPosition, FieldType};

/// FRAMES 帧连续字节流 (字段值随帧变化, 解码值不恒定)
const FRAMES: u16 = 2_048;

fn header(id: &str, hex: &str) -> DecoderBlockDef {
    DecoderBlockDef::Header {
        id: id.to_string(),
        hex: hex.to_string(),
        match_id: None,
    }
}

fn tail(id: &str, hex: &str) -> DecoderBlockDef {
    DecoderBlockDef::Tail {
        id: id.to_string(),
        hex: hex.to_string(),
        match_id: None,
    }
}

fn field(id: &str, ft: FieldType, port: &str) -> DecoderBlockDef {
    DecoderBlockDef::Field {
        id: id.to_string(),
        field_type: ft,
        port_name: port.to_string(),
        length_ref: None,
        match_id: None,
    }
}

fn checksum(id: &str) -> DecoderBlockDef {
    DecoderBlockDef::Checksum {
        id: id.to_string(),
        algorithm: ChecksumAlgorithm::Sum8,
        custom_script: None,
        cover: DecoderChecksumCover::AllPrior,
        cover_start: None,
        cover_end: None,
        position: DecoderChecksumPosition::Inline,
        match_id: None,
    }
}

/// 用 encode_frame 逐帧编码拼接字节流 (与运行时连续字节流形态一致)
fn byte_stream(blocks: &[DecoderBlockDef], channels: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for f in 0..FRAMES {
        let mut values = HashMap::new();
        for c in 0..channels {
            let base = u16::try_from(c).unwrap_or(0);
            values.insert(format!("ch{c}"), f32::from(f.wrapping_add(base)));
        }
        out.extend_from_slice(&FrameDecoderTestData::encode_frame(blocks, &values));
    }
    out
}

fn bench_feed(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame_decode");
    let scenarios: [(&str, usize, bool); 3] = [
        ("u16le_4ch", 4, false),
        ("u16le_4ch_sum8", 4, true),
        ("u16le_16ch", 16, false),
    ];
    for (name, channels, with_checksum) in scenarios {
        let mut blocks = vec![header("h1", "AA")];
        for c in 0..channels {
            blocks.push(field(
                &format!("f{c}"),
                FieldType::UInt16LE,
                &format!("ch{c}"),
            ));
        }
        if with_checksum {
            blocks.push(checksum("cs1"));
        }
        blocks.push(tail("t1", "BB"));

        let bytes = byte_stream(&blocks, channels);
        group.throughput(Throughput::Bytes(u64::try_from(bytes.len()).unwrap_or(1)));
        group.bench_function(name, |b| {
            let mut parser = FrameParser::new(blocks.clone(), false, false, false, false);
            b.iter(|| black_box(parser.feed(&bytes, 1_000)));
        });
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .output_directory(std::path::Path::new("../../../target/criterion/frame_decode"))
        .warm_up_time(std::time::Duration::from_secs(1))
        .measurement_time(std::time::Duration::from_secs(2))
        .sample_size(20);
    targets = bench_feed
}
criterion_main!(benches);
