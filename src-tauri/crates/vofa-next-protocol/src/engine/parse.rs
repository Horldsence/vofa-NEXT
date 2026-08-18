//! 输入解析自由函数 — 把用户输入的字符串按格式转为字节

use super::InputFormat;

/// 自动识别输入格式 — 返回应该使用的具体格式 (Hex 或 Ascii)
///
/// 规则: 去除 "0x" / 空白 / 逗号后, 全为十六进制字符且长度为偶数 → Hex, 否则 Ascii
pub fn detect_format(input: &str) -> InputFormat {
    let no_prefix = input.replace("0x", "").replace("0X", "");
    let clean: String = no_prefix
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',')
        .collect();
    if clean.is_empty() {
        return InputFormat::Ascii;
    }
    if !clean.len().is_multiple_of(2) {
        return InputFormat::Ascii;
    }
    if clean.chars().all(|c| c.is_ascii_hexdigit()) {
        InputFormat::Hex
    } else {
        InputFormat::Ascii
    }
}

/// 解析 HEX 字符串为字节 — 兼容 "AA 01" / "AA01" / "AA,01" / "0xAA 0x01"
pub fn parse_hex(input: &str) -> Result<Vec<u8>, String> {
    // 先去掉所有 "0x" / "0X" 前缀, 再过滤空白与逗号
    let no_prefix = input.replace("0x", "").replace("0X", "");
    let clean: String = no_prefix
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',')
        .collect();
    if clean.is_empty() {
        return Ok(Vec::new());
    }
    if !clean.len().is_multiple_of(2) {
        return Err(format!(
            "HEX 长度必须为偶数 (每字节 2 个十六进制字符), 当前长度 {}",
            clean.len()
        ));
    }
    let mut bytes = Vec::with_capacity(clean.len() / 2);
    let chars: Vec<char> = clean.chars().collect();
    for i in (0..chars.len()).step_by(2) {
        let s: String = chars[i..i + 2].iter().collect();
        let b = u8::from_str_radix(&s, 16).map_err(|_| format!("无效的 HEX 字节: {}", s))?;
        bytes.push(b);
    }
    Ok(bytes)
}

/// 解析 ASCII 文本 + 转义字符 (\n \r \t \xHH \0 \\)
pub fn parse_ascii(input: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            match next {
                'n' => {
                    bytes.push(0x0a);
                    i += 2;
                }
                'r' => {
                    bytes.push(0x0d);
                    i += 2;
                }
                't' => {
                    bytes.push(0x09);
                    i += 2;
                }
                '\\' => {
                    bytes.push(0x5c);
                    i += 2;
                }
                '0' => {
                    bytes.push(0x00);
                    i += 2;
                }
                'x' if i + 3 < chars.len() => {
                    let hex: String = chars[i + 2..i + 4].iter().collect();
                    if let Ok(b) = u8::from_str_radix(&hex, 16) {
                        bytes.push(b);
                        i += 4;
                    } else {
                        bytes.push(ch as u8);
                        i += 1;
                    }
                }
                _ => {
                    bytes.push(ch as u8);
                    i += 1;
                }
            }
        } else {
            // 非 ASCII 字符 (>127) 用 UTF-8 编码
            let s: String = ch.to_string();
            for b in s.bytes() {
                bytes.push(b);
            }
            i += 1;
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_format_hex() {
        assert_eq!(detect_format("AA 01 02 BB"), InputFormat::Hex);
        assert_eq!(detect_format("AA0102BB"), InputFormat::Hex);
        assert_eq!(detect_format("0xAA 0x01"), InputFormat::Hex);
        assert_eq!(detect_format("AA,01,02"), InputFormat::Hex);
    }

    #[test]
    fn test_detect_format_ascii() {
        // 包含非 hex 字符 → Ascii
        assert_eq!(detect_format("1.0,2.0,3.0\\n"), InputFormat::Ascii);
        assert_eq!(detect_format("Hello"), InputFormat::Ascii);
        assert_eq!(detect_format("t1234\\r"), InputFormat::Ascii);
        // 奇数长度 → Ascii
        assert_eq!(detect_format("ABC"), InputFormat::Ascii);
        // 空 → Ascii (默认)
        assert_eq!(detect_format(""), InputFormat::Ascii);
    }

    #[test]
    fn test_parse_hex_spaces() {
        assert_eq!(
            parse_hex("AA 01 02 BB").unwrap(),
            vec![0xAA, 0x01, 0x02, 0xBB]
        );
    }

    #[test]
    fn test_parse_hex_compact() {
        assert_eq!(parse_hex("AA0102BB").unwrap(), vec![0xAA, 0x01, 0x02, 0xBB]);
    }

    #[test]
    fn test_parse_hex_commas() {
        assert_eq!(parse_hex("AA,01,02").unwrap(), vec![0xAA, 0x01, 0x02]);
    }

    #[test]
    fn test_parse_hex_with_0x_prefix() {
        assert_eq!(parse_hex("0xAA 0x01").unwrap(), vec![0xAA, 0x01]);
    }

    #[test]
    fn test_parse_hex_odd_length_error() {
        assert!(parse_hex("ABC").is_err());
    }

    #[test]
    fn test_parse_hex_invalid_char_error() {
        assert!(parse_hex("ZZ").is_err());
    }

    #[test]
    fn test_parse_ascii_plain() {
        assert_eq!(parse_ascii("Hello"), vec![b'H', b'e', b'l', b'l', b'o']);
    }

    #[test]
    fn test_parse_ascii_escapes() {
        assert_eq!(parse_ascii("a\\nb\\nc"), vec![b'a', 0x0a, b'b', 0x0a, b'c']);
        assert_eq!(parse_ascii("\\t\\r\\0\\\\"), vec![0x09, 0x0d, 0x00, 0x5c]);
    }

    #[test]
    fn test_parse_ascii_hex_escape() {
        assert_eq!(parse_ascii("\\xAA\\x01"), vec![0xAA, 0x01]);
    }

    #[test]
    fn test_parse_ascii_utf8() {
        // 中文字符 "中" UTF-8 = E4 B8 AD
        assert_eq!(parse_ascii("中"), vec![0xE4, 0xB8, 0xAD]);
    }
}
