//! 帧 schema 集成测试 — ProtocolSchema + encode_by_blocks + 端口派生。

use schema_types::{
    encode_by_blocks, AsciiBase, ChecksumAlgorithm, DecoderBlockDef, EncodeBlockDef, FieldType,
    ProtocolConfig, ProtocolSchema, SchemaPreset,
};

// ============ SchemaPreset serde ============

#[test]
fn schema_preset_camel_case_serde() {
    assert_eq!(
        serde_json::to_value(SchemaPreset::JustFloat).unwrap(),
        serde_json::json!("justFloat")
    );
    assert_eq!(
        serde_json::to_value(SchemaPreset::FireWater).unwrap(),
        serde_json::json!("fireWater")
    );
    assert_eq!(
        serde_json::to_value(SchemaPreset::CandleLight).unwrap(),
        serde_json::json!("candleLight")
    );
    assert_eq!(
        serde_json::to_value(SchemaPreset::LogicDecode).unwrap(),
        serde_json::json!("logicDecode")
    );
    assert_eq!(
        serde_json::to_value(SchemaPreset::Custom).unwrap(),
        serde_json::json!("custom")
    );
}

#[test]
fn schema_preset_roundtrip() {
    for p in [
        SchemaPreset::JustFloat,
        SchemaPreset::FireWater,
        SchemaPreset::RawData,
        SchemaPreset::Slcan,
        SchemaPreset::CandleLight,
        SchemaPreset::LogicDecode,
        SchemaPreset::Custom,
    ] {
        let json = serde_json::to_string(&p).unwrap();
        let back: SchemaPreset = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }
}

// ============ ProtocolConfig serde / 默认值 ============

#[test]
fn protocol_config_default_is_justfloat() {
    let p = ProtocolConfig::default();
    match p {
        ProtocolConfig::JustFloat { channels: Some(4) } => {}
        _ => panic!("default should be JustFloat{{channels: Some(4)}}"),
    }
}

#[test]
fn protocol_config_serde_tag_kind() {
    let p = ProtocolConfig::JustFloat { channels: Some(4) };
    let j = serde_json::to_value(&p).unwrap();
    assert_eq!(j["kind"], "JustFloat");
    assert_eq!(j["channels"], 4);
}

#[test]
fn protocol_config_logic_decode() {
    let p = ProtocolConfig::LogicDecode {
        decoder: logic_types::LogicDecoderConfig::Uart {
            baud_rate: 115200,
            data_bits: 8,
            parity: vofa_core::Parity::None,
            stop_bits: vofa_core::StopBits::One,
            channel: 0,
        },
    };
    let json = serde_json::to_string(&p).unwrap();
    let back: ProtocolConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(p, back);
}

// ============ ProtocolSchema serde ============

#[test]
fn schema_with_legacy_config_roundtrip() {
    let schema = ProtocolSchema {
        preset: SchemaPreset::JustFloat,
        legacy_config: Some(ProtocolConfig::JustFloat { channels: Some(4) }),
        decode: vec![],
        encode: None,
    };
    let json = serde_json::to_string(&schema).unwrap();
    let back: ProtocolSchema = serde_json::from_str(&json).unwrap();
    assert_eq!(back, schema);
}

#[test]
fn schema_camel_case_field_names() {
    let schema = ProtocolSchema {
        preset: SchemaPreset::Custom,
        legacy_config: None,
        decode: vec![],
        encode: None,
    };
    let j = serde_json::to_value(&schema).unwrap();
    assert_eq!(j["preset"], "custom");
    // None → skip_serializing_if
    assert!(j.get("legacyConfig").is_none());
    assert!(j.get("encode").is_none());
    assert_eq!(j["decode"], serde_json::json!([]));
}

#[test]
fn schema_decode_blocks_serde_with_tag() {
    let schema = ProtocolSchema {
        preset: SchemaPreset::Custom,
        legacy_config: None,
        decode: vec![
            DecoderBlockDef::Csv {
                separator: ",".into(),
                ports: vec!["x".into(), "y".into()],
            },
            DecoderBlockDef::AsciiField {
                port_name: "id".into(),
                base: AsciiBase::Hex,
                digits: 3,
            },
            DecoderBlockDef::Samples {
                decoder: logic_types::LogicDecoderConfig::I2c {
                    sda_channel: 0,
                    scl_channel: 1,
                },
            },
        ],
        encode: None,
    };
    let j = serde_json::to_value(&schema).unwrap();
    assert_eq!(j["decode"][0]["type"], "csv");
    assert_eq!(j["decode"][1]["type"], "asciiField");
    assert_eq!(j["decode"][1]["base"], "hex");
    assert_eq!(j["decode"][2]["type"], "samples");

    let back: ProtocolSchema = serde_json::from_value(j).unwrap();
    assert_eq!(back, schema);
}

#[test]
fn schema_encode_blocks_serde_with_tag_content() {
    let schema = ProtocolSchema {
        preset: SchemaPreset::Custom,
        legacy_config: None,
        decode: vec![],
        encode: Some(vec![
            EncodeBlockDef::VarRef {
                port_name: "a".into(),
                field_type: FieldType::Float32LE,
            },
            EncodeBlockDef::ConstHex {
                hex: "AA BB".into(),
            },
        ]),
    };
    let j = serde_json::to_value(&schema).unwrap();
    assert_eq!(j["encode"][0]["type"], "varRef");
    assert_eq!(j["encode"][0]["params"]["portName"], "a");
    assert_eq!(j["encode"][1]["type"], "constHex");
}

// ============ port_names 派生 ============

#[test]
fn port_names_derivation_with_dedup() {
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
                id: "f0".into(),
                field_type: FieldType::UInt8,
                port_name: "v0".into(),
                length_ref: None,
                match_id: None,
            },
            DecoderBlockDef::Field {
                // 重复 port_name
                id: "f1".into(),
                field_type: FieldType::UInt8,
                port_name: "v0".into(),
                length_ref: None,
                match_id: None,
            },
            DecoderBlockDef::Bitfield {
                id: "b0".into(),
                byte_offset: 1,
                bit_offset: 0,
                bit_length: 4,
                is_signed: false,
                port_name: "flags".into(),
                match_id: None,
            },
            DecoderBlockDef::Csv {
                separator: ",".into(),
                ports: vec!["c0".into(), "c1".into()],
            },
            DecoderBlockDef::AsciiField {
                port_name: "hex_id".into(),
                base: AsciiBase::Hex,
                digits: 2,
            },
        ],
        encode: None,
    };
    let ports = schema.port_names();
    // 去重 + 首次出现顺序
    assert_eq!(ports, vec!["v0", "flags", "c0", "c1", "hex_id"]);
}

// ============ encode_by_blocks ============

#[test]
fn encode_by_blocks_const_hex() {
    let encode = vec![EncodeBlockDef::ConstHex {
        hex: "AA BB CC".into(),
    }];
    let bytes = encode_by_blocks(&encode, &[], &[]);
    assert_eq!(bytes, vec![0xAA, 0xBB, 0xCC]);
}

#[test]
fn encode_by_blocks_var_ref_resolves_port() {
    let encode = vec![
        EncodeBlockDef::VarRef {
            port_name: "a".into(),
            field_type: FieldType::Float32LE,
        },
        EncodeBlockDef::VarRef {
            port_name: "missing".into(), // 不存在 → 0.0
            field_type: FieldType::UInt16BE,
        },
    ];
    let ports = vec!["a".into(), "b".into()];
    let values = [1.5, 99.0];
    let bytes = encode_by_blocks(&encode, &ports, &values);
    let mut expect = Vec::new();
    expect.extend_from_slice(&1.5f32.to_le_bytes());
    expect.extend_from_slice(&0u16.to_be_bytes());
    assert_eq!(bytes, expect);
}

#[test]
fn encode_by_blocks_typed_const() {
    let encode = vec![EncodeBlockDef::TypedConst {
        value: "42".into(),
        field_type: FieldType::UInt8,
    }];
    let bytes = encode_by_blocks(&encode, &[], &[]);
    assert_eq!(bytes, vec![42]);
}

#[test]
fn encode_by_blocks_typed_const_invalid_value_falls_back_to_zero() {
    let encode = vec![EncodeBlockDef::TypedConst {
        value: "not_a_number".into(),
        field_type: FieldType::UInt8,
    }];
    let bytes = encode_by_blocks(&encode, &[], &[]);
    assert_eq!(bytes, vec![0]);
}

#[test]
fn encode_by_blocks_checksum() {
    let encode = vec![
        EncodeBlockDef::ConstHex {
            hex: "01 02".into(),
        },
        EncodeBlockDef::Checksum {
            algorithm: ChecksumAlgorithm::Sum8,
            custom_script: None,
        },
    ];
    let bytes = encode_by_blocks(&encode, &[], &[]);
    assert_eq!(bytes, vec![0x01, 0x02, 0x03]);
}

#[test]
fn encode_by_blocks_full_pipeline() {
    let schema = ProtocolSchema {
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
    };
    let ports = schema.port_names();
    let bytes = encode_by_blocks(schema.encode.as_ref().unwrap(), &ports, &[1.0, 2.0]);
    let mut expect = Vec::new();
    expect.extend_from_slice(&1.0f32.to_le_bytes());
    expect.extend_from_slice(&2.0f32.to_le_bytes());
    expect.extend_from_slice(&[0x00, 0x00, 0x80, 0x7F]);
    assert_eq!(bytes, expect);
}

// ============ PartialEq (手工实现) ============

#[test]
fn partial_eq_respects_decode_and_legacy() {
    let a = ProtocolSchema {
        preset: SchemaPreset::JustFloat,
        legacy_config: Some(ProtocolConfig::JustFloat { channels: Some(4) }),
        decode: vec![DecoderBlockDef::Field {
            id: "f".into(),
            field_type: FieldType::UInt8,
            port_name: "v".into(),
            length_ref: None,
            match_id: None,
        }],
        encode: None,
    };
    // 不同 legacy_config → 不等
    let b = ProtocolSchema {
        legacy_config: Some(ProtocolConfig::JustFloat { channels: Some(8) }),
        ..a.clone()
    };
    assert_ne!(a, b);

    // 不同 decode → 不等
    let c = ProtocolSchema {
        decode: vec![],
        ..a.clone()
    };
    assert_ne!(a, c);

    // 完全相同 → 等
    let d = a.clone();
    assert_eq!(a, d);
}

// ============ TestDataLink ============

#[test]
fn test_data_link_constructor() {
    let link = schema_types::TestDataLink::new(ProtocolConfig::JustFloat { channels: Some(2) });
    assert!(link.schema.is_none());
    match link.protocol {
        ProtocolConfig::JustFloat { channels: Some(2) } => {}
        _ => panic!("protocol 错误"),
    }
}
