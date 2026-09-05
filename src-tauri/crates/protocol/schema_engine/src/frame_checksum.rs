//! 校验和算法 (`ChecksumKind`) + 计算 (`compute_checksum`)
//!
//! 与前端 `checksum.ts` 算法逐位对齐 (CRC-8 / CRC-16 Modbus / CRC-16 CCITT /
//! CRC-32 / Sum8 / Xor8 / LRC); 字节序与多项式系数一致, 任何偏移视为契约漂移。
//!
//! `Custom` 类型 (用户 JS 脚本) 在后端不再支持 — 前端若需保留, 应转为
//! 内置校验类型或迁移到独立 worker。

use serde::{Deserialize, Serialize};

/// 校验类型 — 与前端 `ChecksumKind` (src/lib/utils/checksum.ts) 一一对应。
///
/// 序列化采用前端的字面拼写 (`crc16Modbus` / `crc16CCITT` 大小写混合形式);
/// 全小写形式作为兼容别名同时接受。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChecksumKind {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "sum8")]
    Sum8,
    #[serde(rename = "xor8")]
    Xor8,
    #[serde(rename = "crc8")]
    Crc8,
    #[serde(rename = "crc16Modbus", alias = "crc16modbus")]
    Crc16Modbus,
    #[serde(rename = "crc16CCITT", alias = "crc16ccitt")]
    Crc16Ccitt,
    #[serde(rename = "crc32")]
    Crc32,
    #[serde(rename = "lrc")]
    Lrc,
    /// 自定义 JS 脚本: 后端不支持, 走兜底 (跳过校验) 并返回错误
    #[serde(rename = "custom")]
    Custom,
}

/// CRC-8 (poly 0x07, init 0x00, reflIn/reflOut=false, xorOut=0x00)
fn crc8(data: &[u8]) -> u8 {
    let mut crc: u8 = 0x00;
    for &b in data {
        crc ^= b;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x07
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// CRC-16 Modbus (poly 0xA001, init 0xFFFF, reflIn/reflOut=true, xorOut=0x0000)
fn crc16_modbus(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xffff;
    for &b in data {
        crc ^= u16::from(b);
        for _ in 0..8 {
            crc = if crc & 0x0001 != 0 {
                (crc >> 1) ^ 0xa001
            } else {
                crc >> 1
            };
        }
    }
    crc
}

/// CRC-16 CCITT-FALSE (poly 0x1021, init 0xFFFF, reflIn/reflOut=false, xorOut=0x0000)
fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xffff;
    for &b in data {
        crc ^= u16::from(b) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// CRC-32 (ZIP poly 0xEDB88320 反射, init 0xFFFFFFFF, xorOut=0xFFFFFFFF)
fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xffff_ffff;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    crc ^ 0xffff_ffff
}

/// Sum8 — 累加和 & 0xFF
fn sum8(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |acc, b| acc.wrapping_add(*b))
}

/// Xor8 — 逐字节异或
fn xor8(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |acc, b| acc ^ *b)
}

/// LRC — Modbus ASCII 两倍补码和的低字节
fn lrc(data: &[u8]) -> u8 {
    let s: u8 = data.iter().fold(0u8, |acc, b| acc.wrapping_add(*b));
    // (-s) mod 256 == 补码取反加一, u8 环内即 wrapping_neg
    s.wrapping_neg()
}

/// 计算指定类型的校验和 — 与前端 `computeChecksum` 字节结果一致
///
/// `Custom` 类型: 后端无 JS 沙箱, 返回错误并按空字节兜底。
pub fn compute_checksum(kind: ChecksumKind, data: &[u8]) -> Result<Vec<u8>, String> {
    match kind {
        ChecksumKind::None => Ok(Vec::new()),
        ChecksumKind::Sum8 => Ok(vec![sum8(data)]),
        ChecksumKind::Xor8 => Ok(vec![xor8(data)]),
        ChecksumKind::Crc8 => Ok(vec![crc8(data)]),
        ChecksumKind::Crc16Modbus => {
            let v = crc16_modbus(data);
            Ok(vec![(v & 0xff) as u8, (v >> 8) as u8])
        }
        ChecksumKind::Crc16Ccitt => {
            let v = crc16_ccitt(data);
            Ok(vec![(v >> 8) as u8, (v & 0xff) as u8])
        }
        ChecksumKind::Crc32 => {
            let v = crc32(data);
            Ok(v.to_le_bytes().to_vec())
        }
        ChecksumKind::Lrc => Ok(vec![lrc(data)]),
        ChecksumKind::Custom => Err("Custom checksum script is unsupported on backend".into()),
    }
}

/// 校验和输出字节长度 — `Custom` 按 0 (后端不支持, 兜底为 0 字节)
pub const fn checksum_byte_len(kind: ChecksumKind) -> usize {
    match kind {
        ChecksumKind::None => 0,
        ChecksumKind::Crc32 => 4,
        ChecksumKind::Crc16Modbus | ChecksumKind::Crc16Ccitt => 2,
        ChecksumKind::Sum8
        | ChecksumKind::Xor8
        | ChecksumKind::Crc8
        | ChecksumKind::Lrc
        | ChecksumKind::Custom => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc8_zero_input() {
        assert_eq!(
            compute_checksum(ChecksumKind::Crc8, b"").unwrap(),
            vec![0x00]
        );
    }

    #[test]
    fn crc16_modbus_zero_input() {
        // 0xFFFF in LE = [0xFF, 0xFF]
        assert_eq!(
            compute_checksum(ChecksumKind::Crc16Modbus, b"").unwrap(),
            vec![0xff, 0xff]
        );
    }

    #[test]
    fn crc16_ccitt_zero_input() {
        // 0xFFFF in BE = [0xFF, 0xFF]
        assert_eq!(
            compute_checksum(ChecksumKind::Crc16Ccitt, b"").unwrap(),
            vec![0xff, 0xff]
        );
    }

    #[test]
    fn sum8_basic() {
        assert_eq!(
            compute_checksum(ChecksumKind::Sum8, &[0x01, 0x02]).unwrap(),
            vec![0x03]
        );
        assert_eq!(
            compute_checksum(ChecksumKind::Sum8, &[0xff, 0x01]).unwrap(),
            vec![0x00]
        );
    }

    #[test]
    fn xor8_basic() {
        assert_eq!(
            compute_checksum(ChecksumKind::Xor8, &[0xaa, 0x55]).unwrap(),
            vec![0xff]
        );
    }

    #[test]
    fn lrc_basic() {
        // LRC 是 (-sum) & 0xff; sum8([0x01,0x02])=0x03, LRC=0xFD
        assert_eq!(
            compute_checksum(ChecksumKind::Lrc, &[0x01, 0x02]).unwrap(),
            vec![0xfd]
        );
    }

    #[test]
    fn custom_unsupported() {
        assert!(compute_checksum(ChecksumKind::Custom, b"abc").is_err());
    }
}
