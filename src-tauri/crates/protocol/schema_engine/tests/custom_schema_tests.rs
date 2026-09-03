//! 集成测试: SchemaEngine — 自定义 schema 流式解析/编码

use protocol_engine::ProtocolEngine;
use schema_engine::{compile_schema, SchemaEngine};
use schema_types::{
    AsciiBase, ChecksumAlgorithm, DecoderBlockDef, DecoderChecksumCover, DecoderChecksumPosition,
    EncodeBlockDef, FieldType, ProtocolConfig, ProtocolSchema, SchemaPreset,
};

/// JustFloat 等价的自定义 schema (2×float32LE field + tail)
fn justfloat_like_schema() -> ProtocolSchema {
    ProtocolSchema {
        preset: SchemaPreset::Custom,
        legacy_config: None,
        decode: vec![
            DecoderBlockDef::Field {
                id: "f0".into(),
                field_type: FieldType::Float32LE,
                port_name: "a".into(),
                length_ref: None,
                match_id: None,
            },
            DecoderBlockDef::Field {
                id: "f1".into(),
                field_type: FieldType::Float32LE,
                port_name: "b".into(),
                length_ref: None,
                match_id: None,
            },
            DecoderBlockDef::Tail {
                id: "t0".into(),
                hex: "00 00 80 7F".into(),
                match_id: None,
            },
        ],
        encode: Some(vec![
            EncodeBlockDef::VarRef {
                port_name: "a".into(),
                field_type: FieldType::Float32LE,
            },
            EncodeBlockDef::VarRef {
                port_name: "b".into(),
                field_type: FieldType::Float32LE,
            },
            EncodeBlockDef::ConstHex {
                hex: "00 00 80 7F".into(),
            },
        ]),
    }
}

#[test]
fn test_custom_schema_justfloat_equivalent_decode() {
    let mut engine = compile_schema(&justfloat_like_schema());
    let mut data = Vec::new();
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&2.0f32.to_le_bytes());
    data.extend_from_slice(&[0x00, 0x00, 0x80, 0x7F]);

    let frames = engine.feed(&data).frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].channels, vec![1.0, 2.0]);
}

#[test]
fn test_custom_schema_partial_feed() {
    // 跨包截断: 分两次喂入应拼出完整帧 (2×float32LE + tail = 12 字节, 从第 3 字节处切开)
    let mut engine = compile_schema(&justfloat_like_schema());
    let mut data = Vec::new();
    data.extend_from_slice(&1.5f32.to_le_bytes());
    data.extend_from_slice(&2.5f32.to_le_bytes());
    data.extend_from_slice(&[0x00, 0x00, 0x80, 0x7F]);

    assert!(engine.feed(&data[..3]).frames.is_empty());
    let frames = engine.feed(&data[3..]).frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].channels, vec![1.5, 2.5]);
}

#[test]
fn test_custom_schema_encode_roundtrip() {
    // 编码 → 解码 往返 (JustFloat 等价布局)
    let mut engine = compile_schema(&justfloat_like_schema());
    let bytes = engine.encode_channels(&[3.0, 4.0]);
    let mut expect = Vec::new();
    expect.extend_from_slice(&3.0f32.to_le_bytes());
    expect.extend_from_slice(&4.0f32.to_le_bytes());
    expect.extend_from_slice(&[0x00, 0x00, 0x80, 0x7F]);
    assert_eq!(bytes, expect);

    let mut decoder = compile_schema(&justfloat_like_schema());
    let frames = decoder.feed(&bytes).frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].channels, vec![3.0, 4.0]);
}

#[test]
fn test_custom_schema_csv_decode() {
    let schema = ProtocolSchema {
        preset: SchemaPreset::Custom,
        legacy_config: None,
        decode: vec![DecoderBlockDef::Csv {
            separator: ",".into(),
            ports: vec!["x".into(), "y".into()],
        }],
        encode: None,
    };
    let mut engine = compile_schema(&schema);
    let frames = engine.feed(b"1.0,2.0\n").frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].channels, vec![1.0, 2.0]);

    // 多行 + CRLF
    let frames = engine.feed(b"3.5,4.5\r\n5.0,6.0\n").frames;
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].channels, vec![3.5, 4.5]);
    assert_eq!(frames[1].channels, vec![5.0, 6.0]);
}

#[test]
fn test_custom_schema_header_length_field_checksum() {
    // header + length(uint8) + bytes 字段(length_ref) + sum8 校验 + tail
    let schema = ProtocolSchema {
        preset: SchemaPreset::Custom,
        legacy_config: None,
        decode: vec![
            DecoderBlockDef::Header {
                id: "h".into(),
                hex: "AA".into(),
                match_id: None,
            },
            DecoderBlockDef::Length {
                id: "len".into(),
                field_type: FieldType::UInt8,
                port_name: None,
                unit: None,
                match_id: None,
            },
            DecoderBlockDef::Field {
                id: "payload".into(),
                field_type: FieldType::Bytes,
                port_name: "p".into(),
                length_ref: Some("len".into()),
                match_id: None,
            },
            DecoderBlockDef::Checksum {
                id: "cs".into(),
                algorithm: ChecksumAlgorithm::Sum8,
                custom_script: None,
                cover: DecoderChecksumCover::AllPrior,
                cover_start: None,
                cover_end: None,
                position: DecoderChecksumPosition::Inline,
                match_id: None,
            },
            DecoderBlockDef::Tail {
                id: "t".into(),
                hex: "BB".into(),
                match_id: None,
            },
        ],
        encode: None,
    };
    let mut engine = compile_schema(&schema);
    // AA 02 07 08 (sum8: 02+07+08=17=0x11) BB
    let good = [0xAA, 0x02, 0x07, 0x08, 0x11, 0xBB];
    let frames = engine.feed(&good).frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].channels, vec![7.0]); // Bytes 输出第一字节

    // 校验失败: 帧被被跳过
    let bad = [0xAA, 0x02, 0x07, 0x08, 0x12, 0xBB];
    assert!(engine.feed(&bad).frames.is_empty());
}

#[test]
fn test_custom_schema_ascii_field() {
    // Slcan 类: header 'T' + 3 位 hex id + tail '\r'
    let schema = ProtocolSchema {
        preset: SchemaPreset::Custom,
        legacy_config: None,
        decode: vec![
            DecoderBlockDef::Header {
                id: "h".into(),
                hex: "54".into(), // 'T'
                match_id: None,
            },
            DecoderBlockDef::AsciiField {
                port_name: "id".into(),
                base: AsciiBase::Hex,
                digits: 3,
            },
            DecoderBlockDef::Tail {
                id: "t".into(),
                hex: "0D".into(), // '\r'
                match_id: None,
            },
        ],
        encode: None,
    };
    let mut engine = compile_schema(&schema);
    let frames = engine.feed(b"T1A3\r").frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].channels, vec![f32::from(0x1A3_u16)]);
}

#[test]
fn test_custom_schema_resync_after_garbage() {
    // 垃圾前缀 + 假 header 后能重新同步到真帧
    let schema = ProtocolSchema {
        preset: SchemaPreset::Custom,
        legacy_config: None,
        decode: vec![
            DecoderBlockDef::Header {
                id: "h".into(),
                hex: "AA".into(),
                match_id: None,
            },
            DecoderBlockDef::Field {
                id: "f".into(),
                field_type: FieldType::UInt8,
                port_name: "v".into(),
                length_ref: None,
                match_id: None,
            },
            DecoderBlockDef::Tail {
                id: "t".into(),
                hex: "BB".into(),
                match_id: None,
            },
        ],
        encode: None,
    };
    let mut engine = compile_schema(&schema);
    // 垃圾 + 假 header (AA 后 tail 不匹配) + 真帧
    let data = [0x00, 0xAA, 0x11, 0x22, 0xAA, 0x2A, 0xBB];
    let frames = engine.feed(&data).frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].channels, vec![42.0]);
}

#[test]
fn test_custom_schema_preset_returns_legacy_engine() {
    // 预设路径: legacy_config 存在 → 对应 legacy 引擎
    let schema = ProtocolSchema {
        preset: SchemaPreset::JustFloat,
        legacy_config: Some(ProtocolConfig::JustFloat { channels: Some(2) }),
        decode: vec![],
        encode: None,
    };
    let engine = compile_schema(&schema);
    assert_eq!(engine.name(), "JustFloat");

    // legacy_config 缺失 → 按预设兜底 (FireWater 自动模式)
    let schema = ProtocolSchema {
        preset: SchemaPreset::FireWater,
        legacy_config: None,
        decode: vec![],
        encode: None,
    };
    let engine = compile_schema(&schema);
    assert_eq!(engine.name(), "FireWater");
    assert!(engine.is_auto_mode());

    let schema = ProtocolSchema {
        preset: SchemaPreset::Slcan,
        legacy_config: None,
        decode: vec![],
        encode: None,
    };
    assert_eq!(compile_schema(&schema).name(), "Slcan");

    let schema = ProtocolSchema {
        preset: SchemaPreset::CandleLight,
        legacy_config: None,
        decode: vec![],
        encode: None,
    };
    assert_eq!(compile_schema(&schema).name(), "CandleLight");

    let schema = ProtocolSchema {
        preset: SchemaPreset::RawData,
        legacy_config: None,
        decode: vec![],
        encode: None,
    };
    assert_eq!(compile_schema(&schema).name(), "RawData");
}

#[test]
fn test_schema_engine_name() {
    let schema = justfloat_like_schema();
    let engine = SchemaEngine::new(schema);
    assert_eq!(engine.name(), "CustomSchema");
}

#[test]
fn test_encode_channels_no_encode_blocks_returns_empty() {
    // Custom schema 未定义 encode 块: encode_channels 返回空
    let schema = ProtocolSchema {
        preset: SchemaPreset::Custom,
        legacy_config: None,
        decode: vec![],
        encode: None,
    };
    let mut engine = SchemaEngine::new(schema);
    assert!(engine.encode_channels(&[1.0, 2.0]).is_empty());
}

#[test]
fn test_encode_channel_pads_to_port_count() {
    // encode_channel(channel, value) 构造 values = [0,...,value, 0,...]
    let schema = justfloat_like_schema();
    let mut engine = SchemaEngine::new(schema);
    let bytes = engine.encode_channel(0, 5.0);
    // ports = ["a", "b"], channel 0 = "a"
    let mut expect = Vec::new();
    expect.extend_from_slice(&5.0f32.to_le_bytes());
    expect.extend_from_slice(&0.0f32.to_le_bytes());
    expect.extend_from_slice(&[0x00, 0x00, 0x80, 0x7F]);
    assert_eq!(bytes, expect);
}

#[test]
fn test_new_worker_creates_independent_instance() {
    let schema = justfloat_like_schema();
    let mut engine = SchemaEngine::new(schema);
    let mut worker = engine.new_worker();
    // worker 独立, 喂入数据后原 engine 不受影响
    let mut data = Vec::new();
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&2.0f32.to_le_bytes());
    data.extend_from_slice(&[0x00, 0x00, 0x80, 0x7F]);
    let _ = worker.feed(&data);
    // 原 engine 重新 feed 仍正常
    let frames = engine.feed(&data).frames;
    assert_eq!(frames.len(), 1);
}

#[test]
fn test_take_pending_returns_buffer() {
    let schema = justfloat_like_schema();
    let mut engine = SchemaEngine::new(schema);
    // 喂入不完整数据, 保留在 buf
    let mut data = Vec::new();
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&2.0f32.to_le_bytes());
    let _ = engine.feed(&data);
    let pending = engine.take_pending();
    assert!(!pending.is_empty());
}

#[test]
fn test_bitfield_block_decode() {
    // header + 2 字节 (含 bitfield) + tail
    let schema = ProtocolSchema {
        preset: SchemaPreset::Custom,
        legacy_config: None,
        decode: vec![
            DecoderBlockDef::Header {
                id: "h".into(),
                hex: "AA".into(),
                match_id: None,
            },
            DecoderBlockDef::Bitfield {
                id: "bf".into(),
                byte_offset: 0,
                bit_offset: 0,
                bit_length: 4,
                is_signed: false,
                port_name: "nibble".into(),
                match_id: None,
            },
            DecoderBlockDef::Tail {
                id: "t".into(),
                hex: "BB".into(),
                match_id: None,
            },
        ],
        encode: None,
    };
    let mut engine = compile_schema(&schema);
    // AA BB BB → 头部 AA, bitfield 读 data[1]=BB 高 4 位 = 0xB = 11,
    // bitfield 不消耗 cursor, Tail 检查 data[1] = BB 匹配
    let data = [0xAA, 0xBB, 0xBB];
    let frames = engine.feed(&data).frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].channels, vec![11.0]);
}
