//! 集成测试: `parse_hex` / `parse_ascii` / `detect_format`

use protocol_engine::{detect_format, parse_ascii, parse_hex, InputFormat};

// ===== detect_format =====

#[test]
fn detect_hex_with_spaces() {
    assert_eq!(detect_format("AA 01 02 BB"), InputFormat::Hex);
}

#[test]
fn detect_hex_compact() {
    assert_eq!(detect_format("AA0102BB"), InputFormat::Hex);
}

#[test]
fn detect_hex_with_0x_prefix() {
    assert_eq!(detect_format("0xAA 0x01"), InputFormat::Hex);
}

#[test]
fn detect_hex_with_commas() {
    assert_eq!(detect_format("AA,01,02"), InputFormat::Hex);
}

#[test]
fn detect_ascii_when_non_hex_chars() {
    assert_eq!(detect_format("1.0,2.0,3.0\\n"), InputFormat::Ascii);
    assert_eq!(detect_format("Hello"), InputFormat::Ascii);
    assert_eq!(detect_format("t1234\\r"), InputFormat::Ascii);
}

#[test]
fn detect_ascii_when_odd_length() {
    assert_eq!(detect_format("ABC"), InputFormat::Ascii);
}

#[test]
fn detect_ascii_when_empty() {
    assert_eq!(detect_format(""), InputFormat::Ascii);
}

// ===== parse_hex =====

#[test]
fn parse_hex_with_spaces() {
    assert_eq!(
        parse_hex("AA 01 02 BB").unwrap(),
        vec![0xAA, 0x01, 0x02, 0xBB]
    );
}

#[test]
fn parse_hex_compact() {
    assert_eq!(parse_hex("AA0102BB").unwrap(), vec![0xAA, 0x01, 0x02, 0xBB]);
}

#[test]
fn parse_hex_with_commas() {
    assert_eq!(parse_hex("AA,01,02").unwrap(), vec![0xAA, 0x01, 0x02]);
}

#[test]
fn parse_hex_with_0x_prefix() {
    assert_eq!(parse_hex("0xAA 0x01").unwrap(), vec![0xAA, 0x01]);
}

#[test]
fn parse_hex_lowercase() {
    assert_eq!(parse_hex("aa bb").unwrap(), vec![0xAA, 0xBB]);
}

#[test]
fn parse_hex_empty_returns_empty_vec() {
    assert_eq!(parse_hex("").unwrap(), Vec::<u8>::new());
    assert_eq!(parse_hex("   ").unwrap(), Vec::<u8>::new());
}

#[test]
fn parse_hex_odd_length_error() {
    let err = parse_hex("ABC").unwrap_err();
    assert!(err.contains("偶数"), "expected 偶数 in error, got: {}", err);
}

#[test]
fn parse_hex_invalid_char_error() {
    let err = parse_hex("ZZ").unwrap_err();
    assert!(err.contains("ZZ"), "expected ZZ in error, got: {}", err);
}

// ===== parse_ascii =====

#[test]
fn parse_ascii_plain() {
    assert_eq!(parse_ascii("Hello"), vec![b'H', b'e', b'l', b'l', b'o']);
}

#[test]
fn parse_ascii_escapes_basic() {
    assert_eq!(parse_ascii("a\\nb\\nc"), vec![b'a', 0x0a, b'b', 0x0a, b'c']);
    assert_eq!(parse_ascii("\\t\\r\\0\\\\"), vec![0x09, 0x0d, 0x00, 0x5c]);
}

#[test]
fn parse_ascii_hex_escape() {
    assert_eq!(parse_ascii("\\xAA\\x01"), vec![0xAA, 0x01]);
}

#[test]
fn parse_ascii_hex_escape_invalid_falls_back() {
    // \xZZ 不是合法 hex, 退回原始字符 '\' 与 'x'
    assert_eq!(parse_ascii("\\xZZ"), vec![b'\\', b'x', b'Z', b'Z']);
}

#[test]
fn parse_ascii_utf8_multibyte() {
    // 中文字符 "中" UTF-8 = E4 B8 AD
    assert_eq!(parse_ascii("中"), vec![0xE4, 0xB8, 0xAD]);
}

#[test]
fn parse_ascii_unknown_escape_passes_through() {
    // \q 不识别, 原样输出 '\\' 'q'
    assert_eq!(parse_ascii("\\q"), vec![b'\\', b'q']);
}

#[test]
fn parse_ascii_trailing_backslash_kept() {
    // 末尾单独的 '\' 不构成转义, 原样保留
    assert_eq!(parse_ascii("ab\\"), vec![b'a', b'b', b'\\']);
}