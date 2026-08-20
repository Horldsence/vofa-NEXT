//! FrameParser 核心测试 — parse_hex / 校验算法 / 状态机端到端解析

use node_frame_decoder::{parse_hex, ChecksumAlgorithm, FrameParser};
use schema_types::{DecoderBlockDef, DecoderChecksumCover, DecoderChecksumPosition, FieldType, LengthUnit};

#[test]
fn test_parse_hex_spaces() {
    assert_eq!(parse_hex("AA BB"), vec![0xAA, 0xBB]);
    assert_eq!(parse_hex("AABB"), vec![0xAA, 0xBB]);
    assert_eq!(parse_hex("aa bb"), vec![0xAA, 0xBB]);
    assert_eq!(parse_hex("0xAA 0xBB"), vec![0xAA, 0xBB]);
}

#[test]
fn test_parse_hex_invalid() {
    assert_eq!(parse_hex("AAB"), Vec::<u8>::new()); // 奇数长度
    assert_eq!(parse_hex("ZZ"), Vec::<u8>::new()); // 非法字符
}

#[test]
fn test_checksum_sum8() {
    let data = [0x01, 0x02, 0x03];
    let cs = ChecksumAlgorithm::Sum8.compute(&data, None);
    assert_eq!(cs, vec![0x06]); // 1+2+3=6
}

#[test]
fn test_checksum_xor8() {
    let data = [0x01, 0x02, 0x03];
    let cs = ChecksumAlgorithm::Xor8.compute(&data, None);
    assert_eq!(cs, vec![0x00]); // 1^2^3=0
}

#[test]
fn test_checksum_crc8() {
    // CRC-8/SMBUS: poly=0x07, init=0x00
    // "123456789" → 0xF4
    let data = b"123456789";
    let cs = ChecksumAlgorithm::Crc8.compute(data, None);
    assert_eq!(cs, vec![0xF4]);
}

#[test]
fn test_checksum_crc16_modbus() {
    // CRC-16/Modbus: "123456789" → 0x4B37
    let data = b"123456789";
    let cs = ChecksumAlgorithm::Crc16Modbus.compute(data, None);
    assert_eq!(cs, vec![0x37, 0x4B]); // LE
}

#[test]
fn test_checksum_crc16_ccitt() {
    // CRC-16/CCITT-FALSE: "123456789" → 0x29B1
    let data = b"123456789";
    let cs = ChecksumAlgorithm::Crc16CCITT.compute(data, None);
    assert_eq!(cs, vec![0x29, 0xB1]); // BE
}

#[test]
fn test_checksum_crc32() {
    // CRC-32/ISO-HDLC: "123456789" → 0xCBF43926
    let data = b"123456789";
    let cs = ChecksumAlgorithm::Crc32.compute(data, None);
    assert_eq!(cs, vec![0x26, 0x39, 0xF4, 0xCB]); // LE
}

#[test]
fn test_checksum_lrc() {
    // LRC: 0 - sum(data) mod 256
    let data = [0x01, 0x02, 0x03];
    let cs = ChecksumAlgorithm::Lrc.compute(&data, None);
    // 0 - 6 = 0xFA (mod 256, 二补码)
    assert_eq!(cs, vec![0xFA]);
}

#[test]
fn test_checksum_verify() {
    let data = [0x01, 0x02, 0x03];
    // sum8 = 0x06
    assert!(ChecksumAlgorithm::Sum8.verify(&data, &[0x06], None));
    assert!(!ChecksumAlgorithm::Sum8.verify(&data, &[0x07], None));
}

#[test]
fn test_fps_empty() {
    let p = FrameParser::new(Vec::new(), false, false, false, false);
    assert_eq!(p.fps(), 0.0);
}

#[test]
fn test_matches_config() {
    let blocks = vec![];
    let p = FrameParser::new(blocks.clone(), true, false, false, false);
    assert!(p.matches_config(&blocks, true, false, false, false));
    assert!(!p.matches_config(&blocks, false, false, false, false));
}

// ============ FrameParser 端到端测试 ============

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

#[test]
fn test_parse_fixed_length_frame() {
    // 帧: AA <uint16LE 0x0102> <uint16LE 0x0304> BB
    // 字节: AA 02 01 04 03 BB
    let blocks = vec![
        header("h1", "AA"),
        field("f1", FieldType::UInt16LE, "ch0"),
        field("f2", FieldType::UInt16LE, "ch1"),
        tail("t1", "BB"),
    ];
    let parser = FrameParser::new(blocks, false, false, false, false);

    let data = [0xAA, 0x02, 0x01, 0x04, 0x03, 0xBB];
    let result = parser.parse_once(&data, 1000).expect("应解析成功");

    assert_eq!(result.outputs.get("ch0"), Some(&258.0)); // 0x0102 = 258
    assert_eq!(result.outputs.get("ch1"), Some(&772.0)); // 0x0304 = 772
    assert!(result.valid);
    assert_eq!(result.id_value, None);
}

#[test]
fn test_parse_with_checksum_sum8() {
    // 帧: AA <uint8 0x01> <sum8: 0x01> BB
    // sum8(0x01) = 0x01
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
    let parser = FrameParser::new(blocks, false, false, false, false);

    // 正确校验: AA 01 01 BB
    let data_ok = [0xAA, 0x01, 0x01, 0xBB];
    let result = parser.parse_once(&data_ok, 1000).expect("应解析成功");
    assert!(result.valid);
    assert_eq!(result.outputs.get("value"), Some(&1.0));

    // 错误校验: AA 01 02 BB (sum8 应为 0x01, 实际为 0x02)
    let data_bad = [0xAA, 0x01, 0x02, 0xBB];
    let result_bad = parser.parse_once(&data_bad, 1000).expect("应解析成功");
    assert!(!result_bad.valid);
}

#[test]
fn test_parse_variable_length_frame() {
    // 帧: AA <uint8 length=N> <N bytes data> BB
    // 示例: AA 03 11 22 33 BB (length=3, data=[0x11, 0x22, 0x33])
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
    let parser = FrameParser::new(blocks, false, false, false, false);

    let data = [0xAA, 0x03, 0x11, 0x22, 0x33, 0xBB];
    let result = parser.parse_once(&data, 1000).expect("应解析成功");
    assert_eq!(result.outputs.get("length"), Some(&3.0));
    // Bytes 类型输出第一字节
    assert_eq!(result.outputs.get("data"), Some(&17.0)); // 0x11 = 17
}

#[test]
fn test_parse_multi_frame_dispatch() {
    // 帧: AA <uint8 id> <uint8 value> BB
    // id=1: value 输出到 "type_a" 端口
    // id=2: value 输出到 "type_b" 端口
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
    let parser = FrameParser::new(blocks, false, false, false, false);

    // id=1 帧: AA 01 42 BB → type_a=0x42=66
    let data_a = [0xAA, 0x01, 0x42, 0xBB];
    let result_a = parser.parse_once(&data_a, 1000).expect("应解析成功");
    assert_eq!(result_a.id_value, Some(1));
    assert_eq!(result_a.outputs.get("id_value"), Some(&1.0));
    assert_eq!(result_a.outputs.get("type_a"), Some(&66.0));
    assert!(!result_a.outputs.contains_key("type_b"));

    // id=2 帧: AA 02 99 BB → type_b=0x99=153
    let data_b = [0xAA, 0x02, 0x99, 0xBB];
    let result_b = parser.parse_once(&data_b, 2000).expect("应解析成功");
    assert_eq!(result_b.id_value, Some(2));
    assert_eq!(result_b.outputs.get("type_b"), Some(&153.0));
    assert!(!result_b.outputs.contains_key("type_a"));
}

#[test]
fn test_parse_bitfield() {
    // 帧: AA <byte 0xAB=10101011> BB
    // Bitfield 不消耗 cursor, byte_offset 相对 frame_start (header 之后)
    // 需要一个 Field 块消耗字节, 让 cursor 前进到 Tail 位置
    let blocks = vec![
        header("h1", "AA"),
        // Field 消耗 1 字节 (0xAB), cursor 前进到 2
        field("f1", FieldType::UInt8, "raw_byte"),
        // Bitfield 从 frame_start=1 读取 (相对 header 之后)
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
    let parser = FrameParser::new(blocks, false, false, false, false);

    // AA AB BB → raw_byte=171, high=0xA=10, low=0xB=11
    let data = [0xAA, 0xAB, 0xBB];
    let result = parser.parse_once(&data, 1000).expect("应解析成功");
    assert_eq!(result.outputs.get("raw_byte"), Some(&171.0));
    assert_eq!(result.outputs.get("high_nibble"), Some(&10.0));
    assert_eq!(result.outputs.get("low_nibble"), Some(&11.0));
}

#[test]
fn test_parse_bitfield_signed() {
    // 帧: AA <byte 0xA0=10100000> BB
    // Bitfield: byteOffset=0, bitOffset=0, bitLength=4, signed → 0b1010 = -6 (二补码)
    let blocks = vec![
        header("h1", "AA"),
        field("f1", FieldType::UInt8, "raw_byte"),
        DecoderBlockDef::Bitfield {
            id: "bf1".to_string(),
            byte_offset: 0,
            bit_offset: 0,
            bit_length: 4,
            is_signed: true,
            port_name: "val".to_string(),
            match_id: None,
        },
        tail("t1", "BB"),
    ];
    let parser = FrameParser::new(blocks, false, false, false, false);

    // AA A0 BB → bitfield=0b1010 (4位有符号) = -6
    let data = [0xAA, 0xA0, 0xBB];
    let result = parser.parse_once(&data, 1000).expect("应解析成功");
    assert_eq!(result.outputs.get("val"), Some(&-6.0));
}

#[test]
fn test_feed_multi_frame_in_one_chunk() {
    // 一次喂入两个完整帧, 应解析出 2 个 ParsedFrame
    let blocks = vec![
        header("h1", "AA"),
        field("f1", FieldType::UInt8, "v"),
        tail("t1", "BB"),
    ];
    let mut parser = FrameParser::new(blocks, false, false, false, false);

    // 两帧: AA 01 BB AA 02 BB
    let data = [0xAA, 0x01, 0xBB, 0xAA, 0x02, 0xBB];
    let frames = parser.feed(&data, 1000);
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].outputs.get("v"), Some(&1.0));
    assert_eq!(frames[1].outputs.get("v"), Some(&2.0));
    assert_eq!(parser.frame_count, 2);
}

#[test]
fn test_feed_split_across_chunks() {
    // 帧跨多个数据包到达, 应正确累积解析
    let blocks = vec![
        header("h1", "AA"),
        field("f1", FieldType::UInt16LE, "v"),
        tail("t1", "BB"),
    ];
    let mut parser = FrameParser::new(blocks, false, false, false, false);

    // 第一包: AA 01 (不完整)
    let f1 = parser.feed(&[0xAA, 0x01], 1000);
    assert_eq!(f1.len(), 0);

    // 第二包: 00 BB (完整帧: AA 01 00 BB → v=0x0001=1)
    let f2 = parser.feed(&[0x00, 0xBB], 2000);
    assert_eq!(f2.len(), 1);
    assert_eq!(f2[0].outputs.get("v"), Some(&1.0));
    assert_eq!(parser.frame_count, 1);
}

#[test]
fn test_feed_with_garbage_before_header() {
    // 数据前有垃圾字节, 应自动跳过找到 header
    let blocks = vec![
        header("h1", "AA"),
        field("f1", FieldType::UInt8, "v"),
        tail("t1", "BB"),
    ];
    let mut parser = FrameParser::new(blocks, false, false, false, false);

    // 垃圾 + 完整帧: FF FF FF AA 42 BB
    let data = [0xFF, 0xFF, 0xFF, 0xAA, 0x42, 0xBB];
    let frames = parser.feed(&data, 1000);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].outputs.get("v"), Some(&66.0));
}

#[test]
fn test_parse_no_header() {
    // 无 Header 块 — 直接从开头解析
    let blocks = vec![field("f1", FieldType::UInt8, "v"), tail("t1", "BB")];
    let parser = FrameParser::new(blocks, false, false, false, false);

    let data = [0x42, 0xBB];
    let result = parser.parse_once(&data, 1000).expect("应解析成功");
    assert_eq!(result.outputs.get("v"), Some(&66.0));
}

#[test]
fn test_parse_tail_mismatch_returns_none() {
    // Tail 不匹配 → 返回 None (等待重新查找 header)
    let blocks = vec![
        header("h1", "AA"),
        field("f1", FieldType::UInt8, "v"),
        tail("t1", "BB"),
    ];
    let parser = FrameParser::new(blocks, false, false, false, false);

    // AA 42 CC (Tail 应为 BB, 实际 CC)
    let data = [0xAA, 0x42, 0xCC];
    assert!(parser.parse_once(&data, 1000).is_none());
}
