//! HEX 解析集成测试。

use schema_types::parse_hex;

#[test]
fn parse_hex_simple_space_separated() {
    assert_eq!(parse_hex("AA BB CC"), vec![0xAA, 0xBB, 0xCC]);
}

#[test]
fn parse_hex_no_separator() {
    assert_eq!(parse_hex("AABBCC"), vec![0xAA, 0xBB, 0xCC]);
}

#[test]
fn parse_hex_lowercase() {
    assert_eq!(parse_hex("aa bb"), vec![0xAA, 0xBB]);
}

#[test]
fn parse_hex_mixed_case() {
    assert_eq!(parse_hex("AaBbCc"), vec![0xAA, 0xBB, 0xCC]);
}

#[test]
fn parse_hex_with_0x_prefix() {
    assert_eq!(parse_hex("0xAA 0xBB"), vec![0xAA, 0xBB]);
    assert_eq!(parse_hex("0xAA0xBB"), vec![0xAA, 0xBB]);
}

#[test]
fn parse_hex_with_comma_separator() {
    assert_eq!(parse_hex("AA,BB,CC"), vec![0xAA, 0xBB, 0xCC]);
}

#[test]
fn parse_hex_mixed_separators() {
    assert_eq!(parse_hex("AA, BB 0xCC"), vec![0xAA, 0xBB, 0xCC]);
}

#[test]
fn parse_hex_tabs_and_newlines() {
    assert_eq!(parse_hex("AA\tBB\nCC"), vec![0xAA, 0xBB, 0xCC]);
}

#[test]
fn parse_hex_empty_string() {
    assert!(parse_hex("").is_empty());
}

#[test]
fn parse_hex_whitespace_only() {
    assert!(parse_hex("   \t\n").is_empty());
}

#[test]
fn parse_hex_odd_length_returns_empty() {
    // 奇数长度 → 解析失败 → 返回空 Vec
    assert!(parse_hex("AAB").is_empty());
    assert!(parse_hex("AA BB C").is_empty());
}

#[test]
fn parse_hex_invalid_chars_return_empty() {
    // 非 hex 字符 → Option::None → 整体返回空
    assert!(parse_hex("ZZ").is_empty());
    assert!(parse_hex("AA XX").is_empty());
}

#[test]
fn parse_hex_single_byte() {
    assert_eq!(parse_hex("FF"), vec![0xFF]);
    assert_eq!(parse_hex("00"), vec![0x00]);
}

#[test]
fn parse_hex_decimal_digit_not_allowed() {
    // "9" 是合法 hex 但单字符奇数 → 空
    assert!(parse_hex("9").is_empty());
    assert_eq!(parse_hex("09"), vec![0x09]);
}

#[test]
fn parse_hex_trailing_whitespace_ignored() {
    assert_eq!(parse_hex("AA BB   "), vec![0xAA, 0xBB]);
}
