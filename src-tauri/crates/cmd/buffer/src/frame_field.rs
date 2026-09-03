//! 命令块字段类型 (`FieldType`) + 单字段打包 (`pack_field`)
//!
//! 字节序与小端约定与前端 `commandParser.ts` 的 `packField` 完全一致;
//! 任何偏移视为契约漂移。

use serde::{Deserialize, Serialize};

/// 字段类型 — 与前端 `FieldType` 一一对应 (snake_case 序列化)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    Uint8,
    Int8,
    Uint16Le,
    Uint16Be,
    Int16Le,
    Int16Be,
    Uint32Le,
    Uint32Be,
    Int32Le,
    Int32Be,
    Float32Le,
    Float32Be,
    /// HEX 字节流: `value` 解析为字节序列 (走 parse_hex)
    Bytes,
}

/// 解析十进制 / `0xHEX` / `0bBIN` / 浮点数字字符串
///
/// 浮点输入按向零截断语义取整 (f64→i64 饱和转换); 越界值由下方范围检查统一拒绝
#[allow(clippy::cast_possible_truncation)]
fn parse_number_str(value: &str, min: i64, max: i64) -> Result<i64, String> {
    let trimmed = value.trim();
    let n: i64 = if let Some(rest) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        i64::from_str_radix(rest, 16).map_err(|e| format!("无效的 HEX 数字 `{value}`: {e}"))?
    } else if let Some(rest) = trimmed
        .strip_prefix("0b")
        .or_else(|| trimmed.strip_prefix("0B"))
    {
        i64::from_str_radix(rest, 2).map_err(|e| format!("无效的 BIN 数字 `{value}`: {e}"))?
    } else if trimmed.contains('.') {
        let f: f64 = trimmed
            .parse()
            .map_err(|e| format!("无效的浮点数字 `{value}`: {e}"))?;
        if !f.is_finite() {
            return Err(format!("无效的数字 `{value}`"));
        }
        f as i64
    } else {
        trimmed
            .parse::<i64>()
            .map_err(|e| format!("无效的数字 `{value}`: {e}"))?
    };
    if n < min || n > max {
        return Err(format!("数值 {n} 超出范围 [{min}, {max}]"));
    }
    Ok(n)
}

/// 解析并校验后转换为目标整型 — 范围检查保证 `try_from` 不失败
fn parse_in_range<T: TryFrom<i64>>(value: &str, min: i64, max: i64) -> Result<T, String> {
    let n = parse_number_str(value, min, max)?;
    T::try_from(n).map_err(|_| format!("数值 {n} 超出目标类型范围"))
}

/// HEX 字符串解析 (与前端 parseHex 一致)
/// 接受 `AA 01 02 BB` / `AA0102BB` / `AA,01,02` 等格式
pub fn parse_hex(input: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = input.chars().filter(char::is_ascii_hexdigit).collect();
    if cleaned.is_empty() {
        return Ok(Vec::new());
    }
    if !cleaned.len().is_multiple_of(2) {
        return Err(format!(
            "HEX 长度必须为偶数 (每字节 2 个十六进制字符), 实测 {} 字符",
            cleaned.len()
        ));
    }
    let bytes: Result<Vec<u8>, String> = (0..cleaned.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&cleaned[i..i + 2], 16)
                .map_err(|e| format!("无效的 HEX 字节 `{}`: {e}", &cleaned[i..i + 2]))
        })
        .collect();
    bytes
}

/// 单字段打包 — 与前端 `packField` 逐字节对齐 (同 `FieldType` 同字节序)
pub fn pack_field(field_type: FieldType, value: &str) -> Result<Vec<u8>, String> {
    match field_type {
        FieldType::Uint8 => {
            let n: u8 = parse_in_range(value, 0, 0xff)?;
            Ok(vec![n])
        }
        FieldType::Int8 => {
            let n: i8 = parse_in_range(value, -0x80, 0x7f)?;
            Ok(vec![n.cast_unsigned()])
        }
        FieldType::Uint16Le | FieldType::Uint16Be => {
            let n: u16 = parse_in_range(value, 0, 0xffff)?;
            if matches!(field_type, FieldType::Uint16Le) {
                Ok(n.to_le_bytes().to_vec())
            } else {
                Ok(n.to_be_bytes().to_vec())
            }
        }
        FieldType::Int16Le | FieldType::Int16Be => {
            let n: i16 = parse_in_range(value, -0x8000, 0x7fff)?;
            if matches!(field_type, FieldType::Int16Le) {
                Ok(n.to_le_bytes().to_vec())
            } else {
                Ok(n.to_be_bytes().to_vec())
            }
        }
        FieldType::Uint32Le | FieldType::Uint32Be => {
            let n: u32 = parse_in_range(value, 0, 0xffff_ffff)?;
            if matches!(field_type, FieldType::Uint32Le) {
                Ok(n.to_le_bytes().to_vec())
            } else {
                Ok(n.to_be_bytes().to_vec())
            }
        }
        FieldType::Int32Le | FieldType::Int32Be => {
            let n: i32 = parse_in_range(value, -0x8000_0000, 0x7fff_ffff)?;
            if matches!(field_type, FieldType::Int32Le) {
                Ok(n.to_le_bytes().to_vec())
            } else {
                Ok(n.to_be_bytes().to_vec())
            }
        }
        FieldType::Float32Le | FieldType::Float32Be => {
            let f: f32 = value
                .trim()
                .parse()
                .map_err(|e| format!("无效的浮点数 `{value}`: {e}"))?;
            if !f.is_finite() {
                return Err(format!("无效的浮点数 `{value}`"));
            }
            if matches!(field_type, FieldType::Float32Le) {
                Ok(f.to_le_bytes().to_vec())
            } else {
                Ok(f.to_be_bytes().to_vec())
            }
        }
        FieldType::Bytes => parse_hex(value),
    }
}

/// 拼接多个字节块 (与前端 `concatChunks` 同语义)
pub fn concat_chunks(chunks: &[Vec<u8>]) -> Vec<u8> {
    let total: usize = chunks.iter().map(std::vec::Vec::len).sum();
    let mut out = Vec::with_capacity(total);
    for c in chunks {
        out.extend_from_slice(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_field_uint_le() {
        assert_eq!(
            pack_field(FieldType::Uint16Le, "0x0102").unwrap(),
            vec![0x02, 0x01]
        );
        assert_eq!(
            pack_field(FieldType::Uint16Be, "0x0102").unwrap(),
            vec![0x01, 0x02]
        );
    }

    #[test]
    fn pack_field_int_le() {
        assert_eq!(
            pack_field(FieldType::Int16Le, "-1").unwrap(),
            vec![0xff, 0xff]
        );
    }

    /// 回归: Int32 曾漏转 i64→i32, 错误产出 8 字节 (应为 4 字节)
    #[test]
    fn pack_field_int32_is_four_bytes() {
        assert_eq!(
            pack_field(FieldType::Int32Le, "-2").unwrap(),
            vec![0xfe, 0xff, 0xff, 0xff]
        );
        assert_eq!(
            pack_field(FieldType::Int32Be, "1").unwrap(),
            vec![0x00, 0x00, 0x00, 0x01]
        );
        assert!(pack_field(FieldType::Int32Le, "0x1_0000_0000").is_err());
    }

    #[test]
    fn pack_field_uint8_rejects_overflow() {
        assert!(pack_field(FieldType::Uint8, "256").is_err());
        assert_eq!(pack_field(FieldType::Uint8, "255").unwrap(), vec![0xff]);
        assert_eq!(pack_field(FieldType::Int8, "-128").unwrap(), vec![0x80]);
    }

    #[test]
    fn pack_field_bytes_hex() {
        assert_eq!(
            pack_field(FieldType::Bytes, "AA 01 BB").unwrap(),
            vec![0xaa, 0x01, 0xbb]
        );
    }

    #[test]
    fn parse_hex_odd_len_errors() {
        assert!(parse_hex("AAB").is_err());
    }

    #[test]
    fn concat_chunks_combines() {
        assert_eq!(
            concat_chunks(&[vec![0x01, 0x02], vec![0x03], vec![0x04, 0x05]]),
            vec![0x01, 0x02, 0x03, 0x04, 0x05]
        );
    }
}
