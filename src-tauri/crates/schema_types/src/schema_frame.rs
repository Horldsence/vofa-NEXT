//! 帧 schema 模型 — ProtocolSchema + SchemaPreset + EncodeBlockDef。

use serde::{Deserialize, Serialize};

use crate::decoder_block::DecoderBlockDef;
use crate::hex::parse_hex;
use crate::protocol_config::ProtocolConfig;

/// schema 预设 — 所有现有协议 kind 都是 schema 的预设
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SchemaPreset {
    JustFloat,
    FireWater,
    RawData,
    Slcan,
    CandleLight,
    LogicDecode,
    /// 用户自定义块
    Custom,
}

/// 协议帧 schema — 解析 (decode) 与编码 (encode) 共用同一份定义
///
/// serde camelCase, 与前端 TS `ProtocolSchema` 类型对应。
///
/// PartialEq 为手工实现: legacy_config (ProtocolConfig) 已派生 PartialEq,
/// 沿用应用层惯例用 serde 值比较。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolSchema {
    pub preset: SchemaPreset,
    /// 预设对应的 legacy 引擎配置 (Custom 为 None) — 预设引擎构造用
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_config: Option<ProtocolConfig>,
    /// 解析方向块列表
    #[serde(default)]
    pub decode: Vec<DecoderBlockDef>,
    /// 编码方向块列表 (TestData 生成 / 协议转换用; 预设可为 None = 走 legacy 编码)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encode: Option<Vec<EncodeBlockDef>>,
}

impl PartialEq for ProtocolSchema {
    fn eq(&self, other: &Self) -> bool {
        self.preset == other.preset
            && self.decode == other.decode
            && self.encode == other.encode
            && self.legacy_config == other.legacy_config
    }
}

impl Eq for ProtocolSchema {}

impl ProtocolSchema {
    /// 从 decode 块派生端口列表 (前后端一致的规则):
    /// field.portName / bitfield.portName / csv.ports / asciiField.portName
    /// 按块顺序组成端口列表 (去重, 保持首次出现顺序)。
    ///
    /// 预设 (非 Custom) 端口为 ch0..chN (N 来自 legacy_config.channels 或自动检测),
    /// 不走本派生 — 保持现有行为。
    pub fn port_names(&self) -> Vec<String> {
        let mut ports: Vec<String> = Vec::new();
        for b in &self.decode {
            match b {
                DecoderBlockDef::Field { port_name, .. }
                | DecoderBlockDef::Bitfield { port_name, .. }
                | DecoderBlockDef::AsciiField { port_name, .. } => {
                    if !ports.contains(port_name) {
                        ports.push(port_name.clone());
                    }
                }
                DecoderBlockDef::Csv { ports: ps, .. } => {
                    for p in ps {
                        if !ports.contains(p) {
                            ports.push(p.clone());
                        }
                    }
                }
                _ => {}
            }
        }
        ports
    }
}

/// 编码块定义 (镜像前端 CommandBlock, serde tag="type" content="params" + camelCase)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "type",
    content = "params",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum EncodeBlockDef {
    /// 固定字节序列 (HEX 字符串, 如 "AA BB")
    ConstHex { hex: String },
    /// 引用某端口的运行时值, 按 field_type 编码
    VarRef {
        port_name: String,
        field_type: crate::decoder_block::FieldType,
    },
    /// 字面量常量, 按 field_type 编码 (value 为十进制/浮点字符串)
    TypedConst {
        value: String,
        field_type: crate::decoder_block::FieldType,
    },
    /// 对前序累计字节计算校验并追加
    Checksum {
        algorithm: crate::ChecksumAlgorithm,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        custom_script: Option<String>,
    },
}

/// 按 encode 块列表编码一帧 (SchemaEngine 编码 / TestData 生成共用)
///
/// - `ports`: 端口名列表 (由 schema decode 块派生), VarRef 按名字索引 values
/// - `values`: 与 ports 对齐的运行时值 (越界/缺失按 0.0)
pub fn encode_by_blocks(encode: &[EncodeBlockDef], ports: &[String], values: &[f32]) -> Vec<u8> {
    let mut out = Vec::new();
    for b in encode {
        match b {
            EncodeBlockDef::ConstHex { hex } => {
                out.extend_from_slice(&parse_hex(hex));
            }
            EncodeBlockDef::VarRef {
                port_name,
                field_type,
            } => {
                let v = ports
                    .iter()
                    .position(|p| p == port_name)
                    .and_then(|i| values.get(i))
                    .copied()
                    .unwrap_or(0.0);
                out.extend_from_slice(&field_type.encode(v));
            }
            EncodeBlockDef::TypedConst { value, field_type } => {
                let v: f32 = value.trim().parse().unwrap_or(0.0);
                out.extend_from_slice(&field_type.encode(v));
            }
            EncodeBlockDef::Checksum {
                algorithm,
                custom_script,
            } => {
                let cs = algorithm.compute(&out, custom_script.as_deref());
                out.extend_from_slice(&cs);
            }
        }
    }
    out
}

/// 测试数据链路配置 — TestData 生成器热更新载荷
///
/// 兼容旧的 `ProtocolConfig` 调用方: schema 为 None 或预设时走 legacy 编码;
/// schema 为 Custom 且带 encode 块时按 schema 编码。
#[derive(Debug, Clone)]
pub struct TestDataLink {
    pub protocol: ProtocolConfig,
    pub schema: Option<ProtocolSchema>,
}

impl TestDataLink {
    pub const fn new(protocol: ProtocolConfig) -> Self {
        Self {
            protocol,
            schema: None,
        }
    }
}
