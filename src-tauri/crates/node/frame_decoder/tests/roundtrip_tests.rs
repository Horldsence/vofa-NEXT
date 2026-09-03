//! FrameDecoderTestData 闭环测试
//!
//! encode_frame / encode_frames → FrameParser (parse / feed)
//! 验证编码→解析闭环: 输出值应与输入一致

use frame_decoder::{ChecksumAlgorithm, FrameDecoderTestData, FrameParser};
use schema_types::{
    DecoderBlockDef, DecoderChecksumCover, DecoderChecksumPosition, FieldType, LengthUnit,
};
use std::collections::HashMap;

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

/// 编码固定格式帧, 再用 parser 解析, 验证 round-trip
#[test]
fn test_encode_parse_roundtrip_fixed() {
    let blocks = vec![
        header("h1", "AA"),
        field("f1", FieldType::UInt16LE, "ch0"),
        field("f2", FieldType::UInt16LE, "ch1"),
        tail("t1", "BB"),
    ];
    let mut values = HashMap::new();
    values.insert("ch0".to_string(), 258.0); // 0x0102
    values.insert("ch1".to_string(), 772.0); // 0x0304

    let bytes = FrameDecoderTestData::encode_frame(&blocks, &values);
    assert_eq!(bytes, vec![0xAA, 0x02, 0x01, 0x04, 0x03, 0xBB]);

    let parser = FrameParser::new(blocks, false, false, false, false);
    let result = parser.parse_once(&bytes, 1000).expect("应解析成功");
    assert_eq!(result.outputs.get("ch0"), Some(&258.0));
    assert_eq!(result.outputs.get("ch1"), Some(&772.0));
    assert!(result.valid);
}

/// Checksum Sum8 + Inline 闭环
#[test]
fn test_encode_parse_roundtrip_checksum() {
    let blocks = vec![
        header("h1", "AA"),
        field("f1", FieldType::UInt8, "value"),
        DecoderBlockDef::Checksum {
            id: "cs1".to_string(),
            algorithm: ChecksumAlgorithm::Sum8,
            custom_script: None,
            cover: crate::DecoderChecksumCover::AllPrior,
            cover_start: None,
            cover_end: None,
            position: crate::DecoderChecksumPosition::Inline,
            match_id: None,
        },
        tail("t1", "BB"),
    ];
    let mut values = HashMap::new();
    values.insert("value".to_string(), 42.0);

    let bytes = FrameDecoderTestData::encode_frame(&blocks, &values);
    // AA 2A SUM8 BB → sum8(0x2A) = 0x2A
    assert_eq!(bytes, vec![0xAA, 0x2A, 0x2A, 0xBB]);

    let parser = FrameParser::new(blocks, false, false, false, false);
    let result = parser.parse_once(&bytes, 1000).expect("应解析成功");
    assert!(result.valid);
    assert_eq!(result.outputs.get("value"), Some(&42.0));
}

/// 变长帧 (Length + Bytes) 闭环
#[test]
fn test_encode_parse_roundtrip_variable_length() {
    let blocks = vec![
        header("h1", "AA"),
        DecoderBlockDef::Length {
            id: "len1".to_string(),
            field_type: FieldType::UInt8,
            port_name: Some("length".to_string()),
            unit: Some(crate::LengthUnit::Bytes),
            match_id: None,
        },
        DecoderBlockDef::Field {
            id: "f1".to_string(),
            field_type: FieldType::Bytes,
            port_name: "data".to_string(),
            length_ref: Some("len1".to_string()),
            match_id: None,
        },
        tail("t1", "BB"),
    ];
    let mut values = HashMap::new();
    values.insert("length".to_string(), 3.0);
    values.insert("data".to_string(), 17.0); // 首字节 = 0x11

    let bytes = FrameDecoderTestData::encode_frame(&blocks, &values);
    // AA 03 11 12 13 BB
    assert_eq!(bytes, vec![0xAA, 0x03, 0x11, 0x12, 0x13, 0xBB]);

    let parser = FrameParser::new(blocks, false, false, false, false);
    let result = parser.parse_once(&bytes, 1000).expect("应解析成功");
    assert_eq!(result.outputs.get("length"), Some(&3.0));
    assert_eq!(result.outputs.get("data"), Some(&17.0));
}

/// 多帧分派 (Id + match_id) 闭环
#[test]
fn test_encode_parse_roundtrip_multi_frame() {
    let blocks = vec![
        header("h1", "AA"),
        DecoderBlockDef::Id {
            id: "id1".to_string(),
            field_type: FieldType::UInt8,
            port_name: Some("id_value".to_string()),
        },
        DecoderBlockDef::Field {
            id: "f_a".to_string(),
            field_type: FieldType::UInt8,
            port_name: "type_a".to_string(),
            length_ref: None,
            match_id: Some(1),
        },
        DecoderBlockDef::Field {
            id: "f_b".to_string(),
            field_type: FieldType::UInt8,
            port_name: "type_b".to_string(),
            length_ref: None,
            match_id: Some(2),
        },
        tail("t1", "BB"),
    ];

    // id=1 帧
    let mut values_a = HashMap::new();
    values_a.insert("id_value".to_string(), 1.0);
    values_a.insert("type_a".to_string(), 66.0);
    let bytes_a = FrameDecoderTestData::encode_frame(&blocks, &values_a);
    assert_eq!(bytes_a, vec![0xAA, 0x01, 0x42, 0xBB]);

    // id=2 帧
    let mut values_b = HashMap::new();
    values_b.insert("id_value".to_string(), 2.0);
    values_b.insert("type_b".to_string(), 99.0);
    let bytes_b = FrameDecoderTestData::encode_frame(&blocks, &values_b);
    assert_eq!(bytes_b, vec![0xAA, 0x02, 0x63, 0xBB]);

    let parser = FrameParser::new(blocks, false, false, false, false);
    let result_a = parser.parse_once(&bytes_a, 1000).expect("应解析成功");
    assert_eq!(result_a.id_value, Some(1));
    assert_eq!(result_a.outputs.get("type_a"), Some(&66.0));
    assert!(!result_a.outputs.contains_key("type_b"));

    let result_b = parser.parse_once(&bytes_b, 2000).expect("应解析成功");
    assert_eq!(result_b.id_value, Some(2));
    assert_eq!(result_b.outputs.get("type_b"), Some(&99.0));
    assert!(!result_b.outputs.contains_key("type_a"));
}

/// Bitfield 闭环
#[test]
fn test_encode_parse_roundtrip_bitfield() {
    let blocks = vec![
        header("h1", "AA"),
        field("f1", FieldType::UInt8, "raw_byte"),
        DecoderBlockDef::Bitfield {
            id: "bf1".to_string(),
            byte_offset: 0,
            bit_offset: 0,
            bit_length: 4,
            is_signed: false,
            port_name: "high_nibble".to_string(),
            match_id: None,
        },
        DecoderBlockDef::Bitfield {
            id: "bf2".to_string(),
            byte_offset: 0,
            bit_offset: 4,
            bit_length: 4,
            is_signed: false,
            port_name: "low_nibble".to_string(),
            match_id: None,
        },
        tail("t1", "BB"),
    ];
    let mut values = HashMap::new();
    values.insert("raw_byte".to_string(), 171.0); // 0xAB
    values.insert("high_nibble".to_string(), f32::from(0xA_u8)); // = 10
    values.insert("low_nibble".to_string(), f32::from(0xB_u8)); // = 11

    let bytes = FrameDecoderTestData::encode_frame(&blocks, &values);
    // AA AB BB
    assert_eq!(bytes, vec![0xAA, 0xAB, 0xBB]);

    let parser = FrameParser::new(blocks, false, false, false, false);
    let result = parser.parse_once(&bytes, 1000).expect("应解析成功");
    assert_eq!(result.outputs.get("raw_byte"), Some(&171.0));
    assert_eq!(result.outputs.get("high_nibble"), Some(&10.0));
    assert_eq!(result.outputs.get("low_nibble"), Some(&11.0));
}

/// encode_frames 多帧拼接 → feed 一次多帧
#[test]
fn test_encode_frames_roundtrip() {
    let blocks = vec![
        header("h1", "AA"),
        field("f1", FieldType::UInt8, "v"),
        tail("t1", "BB"),
    ];

    let mut f1 = HashMap::new();
    f1.insert("v".to_string(), 1.0);
    let mut f2 = HashMap::new();
    f2.insert("v".to_string(), 2.0);
    let mut f3 = HashMap::new();
    f3.insert("v".to_string(), 3.0);

    let all_bytes = FrameDecoderTestData::encode_frames(&blocks, &[f1, f2, f3]);
    assert_eq!(
        all_bytes,
        vec![0xAA, 0x01, 0xBB, 0xAA, 0x02, 0xBB, 0xAA, 0x03, 0xBB]
    );

    let mut parser = FrameParser::new(blocks, false, false, false, false);
    let frames = parser.feed(&all_bytes, 1000);
    assert_eq!(frames.len(), 3);
    assert_eq!(frames[0].outputs.get("v"), Some(&1.0));
    assert_eq!(frames[1].outputs.get("v"), Some(&2.0));
    assert_eq!(frames[2].outputs.get("v"), Some(&3.0));
}

/// Checksum 错误检测 (bad_checksum 生成 + 解析验证)
#[test]
fn test_encode_bad_checksum_detected() {
    let blocks = vec![
        header("h1", "AA"),
        field("f1", FieldType::UInt8, "value"),
        DecoderBlockDef::Checksum {
            id: "cs1".to_string(),
            algorithm: ChecksumAlgorithm::Sum8,
            custom_script: None,
            cover: crate::DecoderChecksumCover::AllPrior,
            cover_start: None,
            cover_end: None,
            position: crate::DecoderChecksumPosition::Inline,
            match_id: None,
        },
        tail("t1", "BB"),
    ];

    let mut values = HashMap::new();
    values.insert("value".to_string(), 5.0);

    let bytes_bad = FrameDecoderTestData::encode_frame_bad_checksum(&blocks, &values);
    let parser = FrameParser::new(blocks, false, false, false, false);
    let result = parser.parse_once(&bytes_bad, 1000).expect("应解析成功");
    assert!(!result.valid, "校验字节错误应导致 valid=false");
}

/// encode_frame_with_id 便捷方法
#[test]
fn test_encode_with_id_roundtrip() {
    let blocks = vec![
        header("h1", "AA"),
        DecoderBlockDef::Id {
            id: "id1".to_string(),
            field_type: FieldType::UInt8,
            port_name: Some("id_value".to_string()),
        },
        field("f1", FieldType::UInt8, "value"),
        tail("t1", "BB"),
    ];

    let mut values = HashMap::new();
    values.insert("value".to_string(), 77.0);

    let bytes = FrameDecoderTestData::encode_frame_with_id(&blocks, 3, &values);
    // AA 03 4D BB
    assert_eq!(bytes, vec![0xAA, 0x03, 0x4D, 0xBB]);

    let parser = FrameParser::new(blocks, false, false, false, false);
    let result = parser.parse_once(&bytes, 1000).expect("应解析成功");
    assert_eq!(result.id_value, Some(3));
    assert_eq!(result.outputs.get("id_value"), Some(&3.0));
    assert_eq!(result.outputs.get("value"), Some(&77.0));
}
