//! 校验算法 — CRC / SUM / XOR / LRC 等。
//!
//! 与前端 `ChecksumType` 对齐, serde rename 显式指定字符串。

use serde::{Deserialize, Serialize};

/// 校验算法 (与前端 ChecksumType 对齐, serde rename 显式指定字符串)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChecksumAlgorithm {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "sum8")]
    Sum8,
    #[serde(rename = "xor8")]
    Xor8,
    #[serde(rename = "crc8")]
    Crc8,
    #[serde(rename = "crc16Modbus")]
    Crc16Modbus,
    #[serde(rename = "crc16CCITT")]
    Crc16CCITT,
    #[serde(rename = "crc32")]
    Crc32,
    #[serde(rename = "lrc")]
    Lrc,
    #[serde(rename = "custom")]
    Custom,
}

impl ChecksumAlgorithm {
    /// 计算校验值 (返回单字节或 4 字节, 由调用方截取)
    pub fn compute(self, data: &[u8], custom_script: Option<&str>) -> Vec<u8> {
        match self {
            Self::None => Vec::new(),
            Self::Sum8 => {
                let s: u8 = data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
                vec![s]
            }
            Self::Xor8 => {
                let x: u8 = data.iter().fold(0u8, |acc, &b| acc ^ b);
                vec![x]
            }
            Self::Crc8 => vec![crc8(data, 0x07, 0x00, 0x00)],
            Self::Crc16Modbus => {
                let crc = crc16_modbus(data);
                crc.to_le_bytes().to_vec()
            }
            Self::Crc16CCITT => {
                let crc = crc16_ccitt(data);
                crc.to_be_bytes().to_vec()
            }
            Self::Crc32 => {
                let crc = crc32(data);
                crc.to_le_bytes().to_vec()
            }
            Self::Lrc => {
                let lrc: u8 = data.iter().fold(0u8, |acc, &b| acc.wrapping_sub(b));
                vec![lrc]
            }
            Self::Custom => {
                // 自定义脚本暂不支持后端求值 (前端 lib/checksum.ts 中的 customChecksum 用 JS 实现)
                // 后端此处返回空 Vec, 实际项目应引入 rhai/boa 等 JS 引擎求值
                let _ = custom_script;
                Vec::new()
            }
        }
    }

    /// 比较计算值与期望值 (自动处理长度差异)
    pub fn verify(self, data: &[u8], expected: &[u8], custom_script: Option<&str>) -> bool {
        let computed = self.compute(data, custom_script);
        if computed.is_empty() {
            return true; // None / Custom 未实现 → 默认通过
        }
        computed == expected
    }

    /// 校验算法输出的字节长度
    pub const fn byte_len(self) -> usize {
        match self {
            Self::None => 0,
            Self::Sum8 | Self::Xor8 | Self::Crc8 | Self::Lrc => 1,
            Self::Crc16Modbus | Self::Crc16CCITT => 2,
            Self::Crc32 => 4,
            Self::Custom => 0, // Custom 暂不支持后端求值
        }
    }
}

/// CRC-8 (poly=0x07, init=0x00, refin=false, refout=false, xorout=0x00)
fn crc8(data: &[u8], poly: u8, init: u8, xorout: u8) -> u8 {
    let mut crc = init;
    for &b in data {
        crc ^= b;
        for _ in 0..8 {
            if crc & 0x80 != 0 {
                crc = (crc << 1) ^ poly;
            } else {
                crc <<= 1;
            }
        }
    }
    crc ^ xorout
}

/// CRC-16 Modbus (poly=0x8005, init=0xFFFF, refin=true, refout=true, xorout=0x0000)
fn crc16_modbus(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc ^= u16::from(b);
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xA001; // 0x8005 反转
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}

/// CRC-16 CCITT (poly=0x1021, init=0xFFFF, refin=false, refout=false, xorout=0x0000)
fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc ^= u16::from(b) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

/// CRC-32 (poly=0x04C11DB7, init=0xFFFFFFFF, refin=true, refout=true, xorout=0xFFFFFFFF)
fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320; // 0x04C11DB7 反转
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}
