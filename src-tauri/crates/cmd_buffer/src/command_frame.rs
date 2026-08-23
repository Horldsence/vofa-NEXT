//! 命令发送帧字节打包 (`compute_frame_bytes` 后端单一权威)
//!
//! 与前端 `src/lib/utils/commandFrames.ts::computeFrameBytes` 逐字段对齐:
//! - const_hex: 走 parse_hex
//! - var_ref:   走 pack_field(fieldType, str(inputs[portName]))
//! - typed_const: 走 pack_field(fieldType, value)
//! - checksum:  对前面所有块累计字节计算 (无独立字段长度, 按算法决定)
//!
//! 解码行为不一致 (后端无 JS 沙箱): `Custom` checksum 类型返回错误;
//! Frontend preview 仍是 UI 渲染, 不参与发送控制流。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::frame_checksum::{compute_checksum, ChecksumKind};
use crate::frame_field::{concat_chunks, pack_field, parse_hex, FieldType};

/// 块类型 — 与前端 `BlockType` 一一对应 (snake_case 序列化)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    ConstHex,
    VarRef,
    TypedConst,
    Checksum,
}

/// 单块数据 — `cmd_buffer` 端使用的紧凑表示
///
/// 字段命名对齐前端 CommandBlock:
/// - `hex` / `port_name` / `field_type` / `value` / `checksum` / `custom_script`
/// - `id` 仅前端 UI 用, 后端不上行 (不参与计算)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockDto {
    #[serde(rename = "const_hex")]
    ConstHex {
        hex: Option<String>,
    },
    VarRef {
        port_name: Option<String>,
        field_type: Option<FieldType>,
    },
    TypedConst {
        field_type: Option<FieldType>,
        value: Option<String>,
    },
    Checksum {
        checksum: Option<ChecksumKind>,
        #[serde(default)]
        custom_script: Option<String>,
    },
}

/// 命令帧 DTO — 与前端 `CommandFrame` 对齐 (snake_case)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandFrameDto {
    pub blocks: Vec<BlockDto>,
    #[serde(default)]
    pub append_newline: bool,
}

/// 帧字节打包结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputedFrameDto {
    /// 拼接后的字节流 (None 表示打包失败, 由 `error` 字段说明)
    pub bytes: Option<Vec<u8>>,
    /// 错误信息 (None 表示成功)
    pub error: Option<String>,
    /// 各块的字节片段 (与 blocks 对齐, 失败时为 [])
    pub per_block: Vec<Vec<u8>>,
}

/// 按帧定义拼接字节流 — 后端 IPC 唯一权威
///
/// `inputs` 为 var_ref 端口的 f32 值 (其他类型按 `String::from` 转换)。
pub fn compute_frame_bytes(
    frame: &CommandFrameDto,
    inputs: &HashMap<String, f64>,
) -> ComputedFrameDto {
    let mut chunks: Vec<Vec<u8>> = Vec::with_capacity(frame.blocks.len());
    let mut per_block: Vec<Vec<u8>> = Vec::with_capacity(frame.blocks.len());

    for (idx, block) in frame.blocks.iter().enumerate() {
        let chunk_result: Result<Vec<u8>, String> = match block {
            BlockDto::ConstHex { hex } => parse_hex(hex.as_deref().unwrap_or("")),
            BlockDto::VarRef { port_name, field_type } => {
                let key = port_name.as_deref().unwrap_or("value");
                let val = inputs.get(key).copied().unwrap_or(0.0);
                let ft = field_type.unwrap_or(FieldType::Uint16Le);
                pack_field(ft, &val.to_string())
            }
            BlockDto::TypedConst { field_type, value } => {
                let ft = field_type.unwrap_or(FieldType::Uint8);
                pack_field(ft, value.as_deref().unwrap_or("0"))
            }
            BlockDto::Checksum { checksum, .. } => {
                let kind = checksum.unwrap_or(ChecksumKind::None);
                let prev = concat_chunks(&chunks);
                compute_checksum(kind, &prev)
            }
        };

        match chunk_result {
            Ok(chunk) => {
                chunks.push(chunk.clone());
                per_block.push(chunk);
            }
            Err(e) => {
                return ComputedFrameDto {
                    bytes: None,
                    error: Some(format!("块 #{idx}: {e}")),
                    per_block: Vec::new(),
                };
            }
        }
    }

    let mut result = concat_chunks(&chunks);
    if frame.append_newline {
        result.push(0x0a);
    }
    ComputedFrameDto {
        bytes: Some(result),
        error: None,
        per_block,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_inputs() -> HashMap<String, f64> {
        HashMap::new()
    }

    #[test]
    fn const_hex_basic() {
        let frame = CommandFrameDto {
            blocks: vec![BlockDto::ConstHex {
                hex: Some("AA 01 02".into()),
            }],
            append_newline: false,
        };
        let out = compute_frame_bytes(&frame, &empty_inputs());
        assert!(out.error.is_none(), "{}", out.error.unwrap_or_default());
        assert_eq!(out.bytes.unwrap(), vec![0xaa, 0x01, 0x02]);
    }

    #[test]
    fn var_ref_uint16le_default_zero() {
        let frame = CommandFrameDto {
            blocks: vec![BlockDto::VarRef {
                port_name: Some("speed".into()),
                field_type: Some(FieldType::Uint16Le),
            }],
            append_newline: false,
        };
        let out = compute_frame_bytes(&frame, &empty_inputs());
        assert_eq!(out.bytes.unwrap(), vec![0x00, 0x00]);
    }

    #[test]
    fn typed_const_pack() {
        let frame = CommandFrameDto {
            blocks: vec![BlockDto::TypedConst {
                field_type: Some(FieldType::Uint8),
                value: Some("0x42".into()),
            }],
            append_newline: false,
        };
        let out = compute_frame_bytes(&frame, &empty_inputs());
        assert_eq!(out.bytes.unwrap(), vec![0x42]);
    }

    #[test]
    fn checksum_sum8_over_preceding() {
        let frame = CommandFrameDto {
            blocks: vec![
                BlockDto::ConstHex { hex: Some("01 02".into()) },
                BlockDto::Checksum { checksum: Some(ChecksumKind::Sum8), custom_script: None },
            ],
            append_newline: false,
        };
        let out = compute_frame_bytes(&frame, &empty_inputs());
        assert_eq!(out.bytes.unwrap(), vec![0x01, 0x02, 0x03]);
    }

    #[test]
    fn append_newline_after_chunks() {
        let frame = CommandFrameDto {
            blocks: vec![BlockDto::ConstHex { hex: Some("AA".into()) }],
            append_newline: true,
        };
        let out = compute_frame_bytes(&frame, &empty_inputs());
        assert_eq!(out.bytes.unwrap(), vec![0xaa, 0x0a]);
    }

    #[test]
    fn invalid_hex_reports_error_with_block_index() {
        let frame = CommandFrameDto {
            blocks: vec![
                BlockDto::ConstHex { hex: Some("AA".into()) },
                BlockDto::ConstHex { hex: Some("BADHEX1".into()) },
            ],
            append_newline: false,
        };
        let out = compute_frame_bytes(&frame, &empty_inputs());
        assert!(out.bytes.is_none());
        let err = out.error.unwrap();
        assert!(err.contains("块 #1"), "missing block index: {err}");
    }
}
