//! 校验算法集成测试 — CRC/SUM/XOR/LRC 已知向量 + 边界条件。

use schema_types::ChecksumAlgorithm;

// ============ byte_len ============

#[test]
fn byte_len_matches_variants() {
    assert_eq!(ChecksumAlgorithm::None.byte_len(), 0);
    assert_eq!(ChecksumAlgorithm::Sum8.byte_len(), 1);
    assert_eq!(ChecksumAlgorithm::Xor8.byte_len(), 1);
    assert_eq!(ChecksumAlgorithm::Crc8.byte_len(), 1);
    assert_eq!(ChecksumAlgorithm::Lrc.byte_len(), 1);
    assert_eq!(ChecksumAlgorithm::Crc16Modbus.byte_len(), 2);
    assert_eq!(ChecksumAlgorithm::Crc16CCITT.byte_len(), 2);
    assert_eq!(ChecksumAlgorithm::Crc32.byte_len(), 4);
    assert_eq!(ChecksumAlgorithm::Custom.byte_len(), 0);
}

#[test]
fn byte_len_const_eval() {
    // 编译期 const 评估可用
    const _: usize = ChecksumAlgorithm::Crc32.byte_len();
}

// ============ None / Custom 行为 ============

#[test]
fn none_returns_empty() {
    assert!(ChecksumAlgorithm::None.compute(&[1, 2, 3], None).is_empty());
    assert!(ChecksumAlgorithm::None.verify(&[1, 2, 3], &[0xFF], None));
}

#[test]
fn custom_returns_empty_and_verifies_as_pass() {
    // Custom 暂不支持, 后端返回空 + verify 默认通过
    assert!(ChecksumAlgorithm::Custom
        .compute(b"12345", Some("ignored"))
        .is_empty());
    assert!(ChecksumAlgorithm::Custom.verify(b"12345", &[0xDE, 0xAD], Some("ignored")));
}

// ============ Sum8 ============

#[test]
fn sum8_simple() {
    assert_eq!(ChecksumAlgorithm::Sum8.compute(&[1, 2, 3], None), vec![6]);
}

#[test]
fn sum8_wraps_at_u8_boundary() {
    assert_eq!(ChecksumAlgorithm::Sum8.compute(&[255, 1], None), vec![0]);
    assert_eq!(
        ChecksumAlgorithm::Sum8.compute(&[200, 200], None),
        vec![144]
    );
}

#[test]
fn sum8_empty_data() {
    assert_eq!(ChecksumAlgorithm::Sum8.compute(&[], None), vec![0]);
}

#[test]
fn sum8_verify_match() {
    assert!(ChecksumAlgorithm::Sum8.verify(&[1, 2, 3], &[6], None));
    assert!(!ChecksumAlgorithm::Sum8.verify(&[1, 2, 3], &[7], None));
}

// ============ Xor8 ============

#[test]
fn xor8_simple() {
    assert_eq!(
        ChecksumAlgorithm::Xor8.compute(&[0x0F, 0xF0], None),
        vec![0xFF]
    );
}

#[test]
fn xor8_empty_data() {
    assert_eq!(ChecksumAlgorithm::Xor8.compute(&[], None), vec![0]);
}

#[test]
fn xor8_all_zeros() {
    assert_eq!(
        ChecksumAlgorithm::Xor8.compute(&[0, 0, 0, 0], None),
        vec![0]
    );
}

#[test]
fn xor8_all_ff() {
    assert_eq!(ChecksumAlgorithm::Xor8.compute(&[0xFF; 4], None), vec![0]);
}

// ============ Lrc ============

#[test]
fn lrc_simple_subtraction() {
    // Lrc = 0 - sum (mod 256), wrapping_sub
    assert_eq!(
        ChecksumAlgorithm::Lrc.compute(&[1, 2, 3], None),
        vec![0u8.wrapping_sub(6)]
    );
}

#[test]
fn lrc_empty_data() {
    // 0u8.wrapping_sub(0) == 0
    assert_eq!(ChecksumAlgorithm::Lrc.compute(&[], None), vec![0]);
}

// ============ CRC-8 ============

#[test]
fn crc8_known_vector() {
    // "123456789" → 0xF4 for poly=0x07 init=0x00 (CRC-8/SMBUS 已知向量)
    assert_eq!(
        ChecksumAlgorithm::Crc8.compute(b"123456789", None),
        vec![0xF4]
    );
}

#[test]
fn crc8_empty_data() {
    // init=0x00 xorout=0x00 → 0
    assert_eq!(ChecksumAlgorithm::Crc8.compute(&[], None), vec![0x00]);
}

// ============ CRC-16 Modbus ============

#[test]
fn crc16_modbus_known_vector() {
    // "123456789" → 0x4B37 (Modbus), LE → [0x37, 0x4B]
    assert_eq!(
        ChecksumAlgorithm::Crc16Modbus.compute(b"123456789", None),
        vec![0x37, 0x4B]
    );
}

#[test]
fn crc16_modbus_empty_data() {
    // init=0xFFFF on empty → 0xFFFF, LE → [0xFF, 0xFF]
    assert_eq!(
        ChecksumAlgorithm::Crc16Modbus.compute(&[], None),
        vec![0xFF, 0xFF]
    );
}

// ============ CRC-16 CCITT ============

#[test]
fn crc16_ccitt_known_vector() {
    // "123456789" → 0x29B1 (XMODEM/CCITT-FALSE), BE → [0x29, 0xB1]
    assert_eq!(
        ChecksumAlgorithm::Crc16CCITT.compute(b"123456789", None),
        vec![0x29, 0xB1]
    );
}

// ============ CRC-32 ============

#[test]
fn crc32_known_vector() {
    // "123456789" → 0xCBF43926 (IEEE 802.3), LE → [0x26, 0x39, 0xF4, 0xCB]
    assert_eq!(
        ChecksumAlgorithm::Crc32.compute(b"123456789", None),
        vec![0x26, 0x39, 0xF4, 0xCB]
    );
}

#[test]
fn crc32_empty_data() {
    // IEEE 802.3 init=0xFFFFFFFF xorout=0xFFFFFFFF on empty → 0
    assert_eq!(
        ChecksumAlgorithm::Crc32.compute(&[], None),
        vec![0, 0, 0, 0]
    );
}

#[test]
fn crc32_verify() {
    let expected = vec![0x26, 0x39, 0xF4, 0xCB];
    assert!(ChecksumAlgorithm::Crc32.verify(b"123456789", &expected, None));
    assert!(!ChecksumAlgorithm::Crc32.verify(b"123456789", &[0, 0, 0, 0], None));
}

// ============ serde ============

#[test]
fn serde_rename_strings() {
    assert_eq!(
        serde_json::to_value(ChecksumAlgorithm::None).unwrap(),
        serde_json::json!("none")
    );
    assert_eq!(
        serde_json::to_value(ChecksumAlgorithm::Crc16Modbus).unwrap(),
        serde_json::json!("crc16Modbus")
    );
    assert_eq!(
        serde_json::to_value(ChecksumAlgorithm::Crc16CCITT).unwrap(),
        serde_json::json!("crc16CCITT")
    );
}

#[test]
fn serde_roundtrip_all_variants() {
    for algo in [
        ChecksumAlgorithm::None,
        ChecksumAlgorithm::Sum8,
        ChecksumAlgorithm::Xor8,
        ChecksumAlgorithm::Crc8,
        ChecksumAlgorithm::Crc16Modbus,
        ChecksumAlgorithm::Crc16CCITT,
        ChecksumAlgorithm::Crc32,
        ChecksumAlgorithm::Lrc,
        ChecksumAlgorithm::Custom,
    ] {
        let json = serde_json::to_string(&algo).unwrap();
        let back: ChecksumAlgorithm = serde_json::from_str(&json).unwrap();
        assert_eq!(algo, back);
    }
}
