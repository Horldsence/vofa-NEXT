//! 帧解码块集成测试 — FieldType + DecoderBlockDef serde/方法/边界。

use schema_types::{
    AsciiBase, ChecksumAlgorithm, DecoderBlockDef, DecoderChecksumCover, DecoderChecksumPosition,
    FieldType, LengthUnit,
};
use serde_json::json;

// ============ FieldType::byte_len ============

#[test]
fn field_type_byte_len() {
    assert_eq!(FieldType::UInt8.byte_len(), Some(1));
    assert_eq!(FieldType::Int8.byte_len(), Some(1));
    assert_eq!(FieldType::UInt16LE.byte_len(), Some(2));
    assert_eq!(FieldType::Float32BE.byte_len(), Some(4));
    assert_eq!(FieldType::Bytes.byte_len(), None);
}

// ============ FieldType::decode 边界 ============

#[test]
fn field_type_decode_u8_normal_and_max() {
    assert_eq!(FieldType::UInt8.decode(&[0xFF]), Some(255.0));
    assert_eq!(FieldType::UInt8.decode(&[0x00]), Some(0.0));
    assert_eq!(FieldType::UInt8.decode(&[]), None);
}

#[test]
fn field_type_decode_i8_negative() {
    assert_eq!(FieldType::Int8.decode(&[0xFF]), Some(-1.0));
    assert_eq!(FieldType::Int8.decode(&[0x80]), Some(-128.0));
    assert_eq!(FieldType::Int8.decode(&[0x7F]), Some(127.0));
}

#[test]
fn field_type_decode_uint16_endianness() {
    let bytes = [0x34, 0x12];
    assert_eq!(FieldType::UInt16LE.decode(&bytes), Some(0x1234 as f32));
    assert_eq!(FieldType::UInt16BE.decode(&bytes), Some(0x3412 as f32));
}

#[test]
fn field_type_decode_short_buffer_returns_none() {
    assert_eq!(FieldType::UInt16LE.decode(&[0x01]), None);
    assert_eq!(FieldType::UInt32LE.decode(&[0x01, 0x02]), None);
    assert_eq!(FieldType::Float32BE.decode(&[0x01, 0x02, 0x03]), None);
}

#[test]
fn field_type_decode_int16_sign_extension() {
    assert_eq!(FieldType::Int16BE.decode(&[0xFF, 0xFF]), Some(-1.0));
    assert_eq!(FieldType::Int16LE.decode(&[0xFF, 0xFF]), Some(-1.0));
}

#[test]
fn field_type_decode_float32() {
    let bytes = 1.5f32.to_le_bytes();
    assert_eq!(FieldType::Float32LE.decode(&bytes), Some(1.5));
    let bytes_be = 1.5f32.to_be_bytes();
    assert_eq!(FieldType::Float32BE.decode(&bytes_be), Some(1.5));
}

#[test]
fn field_type_decode_bytes_returns_first_byte() {
    assert_eq!(
        FieldType::Bytes.decode(&[0x42, 0x99, 0xAB]),
        Some(f32::from(0x42u8))
    );
    assert_eq!(FieldType::Bytes.decode(&[]), None);
}

// ============ FieldType::encode 边界 ============

#[test]
fn field_type_encode_uint_truncates() {
    // f32→u8 是 saturating cast: 256.0 → 255 (而非 wrapping 到 0)
    assert_eq!(FieldType::UInt8.encode(256.0), vec![255]);
    assert_eq!(FieldType::UInt8.encode(255.0), vec![255]);
    assert_eq!(FieldType::UInt8.encode(0.0), vec![0]);
}

#[test]
fn field_type_encode_int_signs() {
    assert_eq!(FieldType::Int8.encode(-1.0), vec![0xFF]);
    assert_eq!(FieldType::Int8.encode(127.0), vec![0x7F]);
}

#[test]
fn field_type_encode_float32() {
    let bytes = FieldType::Float32LE.encode(1.5);
    assert_eq!(bytes, 1.5f32.to_le_bytes().to_vec());
}

#[test]
fn field_type_encode_bytes_truncates_low_byte() {
    // Bytes 编码: 单字节 = value as u8 (浮点→整型 saturating cast)
    assert_eq!(FieldType::Bytes.encode(0x34 as f32), vec![0x34]);
    assert_eq!(FieldType::Bytes.encode(300.5), vec![255]); // 超 255 saturating
}

// ============ encode/decode 往返 ============

#[test]
fn field_type_roundtrip_all_fixed_types() {
    for ft in [
        FieldType::UInt8,
        FieldType::Int8,
        FieldType::UInt16LE,
        FieldType::UInt16BE,
        FieldType::Int16LE,
        FieldType::Int16BE,
        FieldType::UInt32LE,
        FieldType::UInt32BE,
        FieldType::Int32LE,
        FieldType::Int32BE,
        FieldType::Float32LE,
        FieldType::Float32BE,
    ] {
        let v = 42.0f32;
        let bytes = ft.encode(v);
        assert_eq!(ft.decode(&bytes), Some(v), "{ft:?} 编解码应往返");
    }
}

// ============ DecoderBlockDef::id / match_id / output_port_name ============

#[test]
fn decoder_block_id_returns_id_for_typed_blocks() {
    let b = DecoderBlockDef::Header {
        id: "h0".into(),
        hex: "AA".into(),
        match_id: None,
    };
    assert_eq!(b.id(), "h0");

    let b = DecoderBlockDef::Csv {
        separator: ",".into(),
        ports: vec!["a".into()],
    };
    assert_eq!(b.id(), ""); // 扩展块无 id
}

#[test]
fn decoder_block_match_id_some_and_none() {
    let b = DecoderBlockDef::Field {
        id: "f0".into(),
        field_type: FieldType::UInt8,
        port_name: "v".into(),
        length_ref: None,
        match_id: Some(42),
    };
    assert_eq!(b.match_id(), Some(42));

    let b = DecoderBlockDef::Id {
        id: "i".into(),
        field_type: FieldType::UInt8,
        port_name: None,
    };
    assert_eq!(b.match_id(), None);
}

#[test]
fn decoder_block_output_port_name_default() {
    let b = DecoderBlockDef::Length {
        id: "l".into(),
        field_type: FieldType::UInt16LE,
        port_name: None,
        unit: None,
        match_id: None,
    };
    assert_eq!(b.output_port_name(), Some("length"));

    let b = DecoderBlockDef::Id {
        id: "i".into(),
        field_type: FieldType::UInt8,
        port_name: None,
    };
    assert_eq!(b.output_port_name(), Some("id_value"));

    let b = DecoderBlockDef::Header {
        id: "h".into(),
        hex: "AA".into(),
        match_id: None,
    };
    assert_eq!(b.output_port_name(), None);
}

// ============ serde 标签 ============

#[test]
fn decoder_block_serde_field_camel_case() {
    let b = DecoderBlockDef::Field {
        id: "f0".into(),
        field_type: FieldType::Float32LE,
        port_name: "ch0".into(),
        length_ref: Some("len".into()),
        match_id: None,
    };
    let j = serde_json::to_value(&b).unwrap();
    assert_eq!(j["type"], "field");
    assert_eq!(j["id"], "f0");
    assert_eq!(j["fieldType"], "float32LE");
    assert_eq!(j["portName"], "ch0");
    assert_eq!(j["lengthRef"], "len");
    assert!(j.get("matchId").is_none()); // skip_serializing_if = None
}

#[test]
fn decoder_block_serde_id_block_no_match_id() {
    let b = DecoderBlockDef::Id {
        id: "i".into(),
        field_type: FieldType::UInt8,
        port_name: None,
    };
    let j = serde_json::to_value(&b).unwrap();
    assert_eq!(j["type"], "id");
    assert!(j.get("portName").is_none()); // None → 跳过
}

#[test]
fn decoder_block_serde_checksum_all_fields() {
    let b = DecoderBlockDef::Checksum {
        id: "c".into(),
        algorithm: ChecksumAlgorithm::Crc16Modbus,
        custom_script: None,
        cover: DecoderChecksumCover::Range,
        cover_start: Some(0),
        cover_end: Some(4),
        position: DecoderChecksumPosition::Inline,
        match_id: Some(1),
    };
    let j = serde_json::to_value(&b).unwrap();
    assert_eq!(j["type"], "checksum");
    assert_eq!(j["algorithm"], "crc16Modbus");
    assert_eq!(j["cover"], "range");
    assert_eq!(j["coverStart"], 0);
    assert_eq!(j["coverEnd"], 4);
    assert_eq!(j["position"], "inline");
    assert_eq!(j["matchId"], 1);
}

#[test]
fn decoder_block_serde_csv() {
    let b = DecoderBlockDef::Csv {
        separator: ",".into(),
        ports: vec!["a".into(), "b".into()],
    };
    let j = serde_json::to_value(&b).unwrap();
    assert_eq!(j["type"], "csv");
    assert_eq!(j["separator"], ",");
    assert_eq!(j["ports"][0], "a");
    assert_eq!(j["ports"][1], "b");
}

#[test]
fn decoder_block_serde_ascii_field() {
    let b = DecoderBlockDef::AsciiField {
        port_name: "id".into(),
        base: AsciiBase::Hex,
        digits: 3,
    };
    let j = serde_json::to_value(&b).unwrap();
    assert_eq!(j["type"], "asciiField");
    assert_eq!(j["base"], "hex");
    assert_eq!(j["digits"], 3);
}

#[test]
fn decoder_block_serde_samples_uses_logic_types() {
    use logic_types::LogicDecoderConfig;
    let b = DecoderBlockDef::Samples {
        decoder: LogicDecoderConfig::Uart {
            baud_rate: 115200,
            data_bits: 8,
            parity: vofa_core::Parity::None,
            stop_bits: vofa_core::StopBits::One,
            channel: 0,
        },
    };
    let j = serde_json::to_value(&b).unwrap();
    assert_eq!(j["type"], "samples");
    assert_eq!(j["decoder"]["kind"], "Uart");
    assert_eq!(j["decoder"]["params"]["baud_rate"], 115200);
}

// ============ 边界: 枚举 PartialEq ============

#[test]
fn decoder_block_partial_eq() {
    let a = DecoderBlockDef::Header {
        id: "h".into(),
        hex: "AA".into(),
        match_id: None,
    };
    let b = DecoderBlockDef::Header {
        id: "h".into(),
        hex: "AA".into(),
        match_id: None,
    };
    assert_eq!(a, b);

    let c = DecoderBlockDef::Header {
        id: "h".into(),
        hex: "BB".into(), // 不同
        match_id: None,
    };
    assert_ne!(a, c);
}

// ============ LengthUnit / AsciiBase 序列化 ============

#[test]
fn length_unit_serde() {
    assert_eq!(
        serde_json::to_value(LengthUnit::Bytes).unwrap(),
        json!("bytes")
    );
    assert_eq!(
        serde_json::to_value(LengthUnit::Fields).unwrap(),
        json!("fields")
    );
}

#[test]
fn ascii_base_serde() {
    assert_eq!(serde_json::to_value(AsciiBase::Hex).unwrap(), json!("hex"));
    assert_eq!(serde_json::to_value(AsciiBase::Dec).unwrap(), json!("dec"));
}
