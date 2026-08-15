//! # vofa-next-nodes
//!
//! 节点图 DAG 引擎 — 后端计算所有节点的输出值。
//!
//! 核心类型:
//! - [`NodeKind`][]: 节点种类 (ChannelSource/Input/Math/Custom/Filter/SpectrumSink/FrameDecoder/Sink)
//! - [`NodeDef`][]: 节点定义 (含 id/tab_id/kind/params)
//! - [`CompiledGraph`]: 编译后的 DAG, 含拓扑序, 提供 evaluate 方法
//!
//! 数据流:
//!   DataFrame → CompiledGraph.evaluate(frame, input_values, custom_outputs, filter_states)
//!            → HashMap<widgetId, HashMap<portId, f32>>  (所有节点的输出)
//!
//! 节点输出约定:
//! - ChannelSource: 输出端口 "ch0", "ch1", ... (帧通道值)
//! - Input: 输出端口 "value" (来自前端 invoke)
//! - Math: 输出端口 "result"
//! - Custom: 输出端口由前端回传 (custom_outputs)
//! - Filter: 输出端口 "result" (逐点滤波, 融入 eval_order)
//! - SpectrumSink: 无输出 (块运算, 独立 30 FPS ticker 触发 FFT, 不在 eval_order)
//! - FrameDecoder: 输出端口来自 blocks 中的 field/bitfield + 可选 valid/frame_count/last_timestamp/fps
//!   (跨帧状态由 data_loop 喂入字节流, 解析结果缓存在 decoder_states 中)
//! - Sink: 无输出 (纯消费, 不在 DAG 中评估)
//!
//! 前端通过 edges 自行解析 Sink 的输入: 上游 widgetId + sourceHandle → 输出快照查值

pub mod frame_decoder;
pub mod math_op;

pub use frame_decoder::{ChecksumAlgorithm, FrameParser, ParsedFrame};
pub use math_op::MathOp;
pub use vofa_next_dsp::{DigitalFilter, FilterKind, FilterPreset, SpectrumOutput, WindowType};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use vofa_next_buffer::graph::Edge;
use vofa_next_core::DataFrame;

/// 图输出值表 (热路径) — FxHash 替代 SipHash, 高码率逐帧覆盖写时查找快 3~5 倍。
/// serde 对任意 BuildHasher+Default 的 HashMap 透明, 线上 JSON 格式不变。
pub type ValuesMap = HashMap<String, HashMap<String, f32, FxBuildHasher>, FxBuildHasher>;
use rustc_hash::FxBuildHasher;

/// 节点种类 — 决定节点如何被评估
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "params")]
pub enum NodeKind {
    /// 通道源 (虚拟, 每个 tab 一个, 输出 ch0..chN)
    /// params: 通道数
    ChannelSource { channels: usize },
    /// 输入控件 (Knob/Slider/Button/Radio/Checkbox)
    /// 输出端口固定 "value", 值来自前端 invoke('set_input_value')
    Input,
    /// 算术节点
    /// 输出端口 "result"
    Math { op: MathOp, input_count: usize },
    /// 自定义 JS 节点
    /// 输入端口由用户代码定义, 输出端口由前端 iframe 回传
    /// 后端使用 custom_outputs 中的值作为节点输出
    Custom {
        /// 输入端口 id 列表 (前端解析代码后告诉后端)
        inputs: Vec<String>,
        /// 输出端口 id 列表
        outputs: Vec<String>,
    },
    /// 数字滤波器节点 (逐点运算, 融入 eval_order)
    /// 输入端口 "in0", 输出端口 "result"
    /// 后端维护滤波器状态 (FIR 延迟线 / IIR biquad 状态), 跨帧持久化
    /// 状态存储在 evaluate 的 filter_states 参数中, 由调用方管理生命周期
    Filter {
        /// 滤波器配置 (FIR coeffs 或 IIR biquad)
        kind: FilterKind,
    },
    /// 频谱分析节点 (块运算, 不在 eval_order)
    /// 输入端口 "in0", 无输出端口
    /// 后端维护滑动窗口, 由独立 30 FPS ticker 触发 FFT, 结果存入 spectrum_snapshot
    /// 通过 collect_spectrum_inputs 在每帧后从 output_snapshot 取输入值推入分析器
    SpectrumSink {
        /// FFT 窗口大小 (建议 2 的幂, 如 256/512/1024/2048)
        window_size: usize,
        /// 窗函数类型
        window_type: WindowType,
        /// 频谱输出模式
        output: SpectrumOutput,
        /// 采样率 (Hz), 用于计算频率轴
        sample_rate: f32,
    },
    /// 帧解码节点 (SOURCE 类型, 输出来自字节流解析)
    ///
    /// 设计动机: 类似 CommandSender 但反向 — 字节流 → 按块定义解析 → 输出端口。
    /// 每个 field/bitfield 块对应一个输出端口, 另有可选 valid/frame_count/last_timestamp/fps 端口。
    ///
    /// 字节来源:
    /// - loopback=false: data_loop 将实时 RX 字节流默认喂入 (无输入端口)
    /// - loopback=true:  显示 loopbackIn 字节输入口, 只接收回环边注入的字节,
    ///   data_loop 的默认喂入被跳过 (见 decoder_feed.rs)
    ///
    /// 跨帧状态: FrameParser 状态机由调用方 (data_loop / inject_loopback_bytes) 管理,
    /// 字节流通过 feed_frame_decoders / feed_one_decoder 推入, 解析完成后输出缓存到 decoder_states,
    /// evaluate 时从缓存读取最近一次解析结果。
    FrameDecoder {
        /// 块列表 (按顺序定义帧布局)
        blocks: Vec<DecoderBlockDef>,
        /// 附加输出端口开关 (与前端 FrameDecoderConfig 对应)
        enable_valid: bool,
        enable_frame_count: bool,
        enable_last_timestamp: bool,
        enable_fps: bool,
        /// 回环模式: 只接收 loopbackIn 回环边注入的字节, 不再默认接收实时 RX
        loopback: bool,
    },
    /// Sink 节点 (Label/Gauge/LED/NumberDisplay/PieChart/Image/Waveform)
    /// 这些节点没有输出, 后端 DAG 不评估它们, 前端通过 edges 自行查值
    Sink,
}

// ============ 帧解码块类型 (FrameDecoder) ============
//
// 与前端 `DecoderBlock` 类型对齐 (src/types/index.ts),
// serde 使用 `tag = "type" content = "params"` 与前端 discriminant 字段 "type" 一致。
//
// 块类型:
// - Header:   匹配帧头固定字节序列 (帧起始标志)
// - Length:   读 N 字节为整数, 输出到 length 端口 + 决定后续变长字段长度
// - Id:       读 N 字节为整数, 输出到 id_value 端口 + 设置 match_id 上下文
// - Field:    按 field_type 读 N 字节并解码为 f32, 输出到 port_name 端口
// - Bitfield: 从指定字节按 bit 偏移+位长读取, 输出到 port_name 端口
// - Checksum: 对前序累计字节校验, 输出 valid 端口 (1.0/0.0)
// - Tail:     匹配帧尾固定字节序列 (可选, 帧结束标志)

/// 整数字段类型 (与前端 FieldType 对应)
///
/// serde rename_all="kebab-case" 与前端 PascalCase 不同 —
/// 这里使用 serde rename 显式指定每个变体的字符串, 确保与前端 TS 联合类型字符串完全一致。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FieldType {
    #[serde(rename = "uint8")]
    UInt8,
    #[serde(rename = "int8")]
    Int8,
    #[serde(rename = "uint16LE")]
    UInt16LE,
    #[serde(rename = "uint16BE")]
    UInt16BE,
    #[serde(rename = "int16LE")]
    Int16LE,
    #[serde(rename = "int16BE")]
    Int16BE,
    #[serde(rename = "uint32LE")]
    UInt32LE,
    #[serde(rename = "uint32BE")]
    UInt32BE,
    #[serde(rename = "int32LE")]
    Int32LE,
    #[serde(rename = "int32BE")]
    Int32BE,
    #[serde(rename = "float32LE")]
    Float32LE,
    #[serde(rename = "float32BE")]
    Float32BE,
    /// 变长字节序列 (长度由 length_ref 决定)
    #[serde(rename = "bytes")]
    Bytes,
}

impl FieldType {
    /// 该字段类型的固定字节长度 (Bytes 返回 None, 需由 length_ref 决定)
    pub fn byte_len(self) -> Option<usize> {
        match self {
            FieldType::UInt8 | FieldType::Int8 => Some(1),
            FieldType::UInt16LE | FieldType::UInt16BE | FieldType::Int16LE | FieldType::Int16BE => {
                Some(2)
            }
            FieldType::UInt32LE
            | FieldType::UInt32BE
            | FieldType::Int32LE
            | FieldType::Int32BE
            | FieldType::Float32LE
            | FieldType::Float32BE => Some(4),
            FieldType::Bytes => None,
        }
    }

    /// 从字节切片解析为 f32 (按字段类型解码)
    /// 长度不足时返回 None
    pub fn decode(self, bytes: &[u8]) -> Option<f32> {
        match self {
            FieldType::UInt8 => bytes.first().map(|&b| b as f32),
            FieldType::Int8 => bytes.first().map(|&b| (b as i8) as f32),
            FieldType::UInt16LE => {
                if bytes.len() < 2 {
                    return None;
                }
                Some(u16::from_le_bytes([bytes[0], bytes[1]]) as f32)
            }
            FieldType::UInt16BE => {
                if bytes.len() < 2 {
                    return None;
                }
                Some(u16::from_be_bytes([bytes[0], bytes[1]]) as f32)
            }
            FieldType::Int16LE => {
                if bytes.len() < 2 {
                    return None;
                }
                Some((i16::from_le_bytes([bytes[0], bytes[1]])) as f32)
            }
            FieldType::Int16BE => {
                if bytes.len() < 2 {
                    return None;
                }
                Some((i16::from_be_bytes([bytes[0], bytes[1]])) as f32)
            }
            FieldType::UInt32LE => {
                if bytes.len() < 4 {
                    return None;
                }
                Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f32)
            }
            FieldType::UInt32BE => {
                if bytes.len() < 4 {
                    return None;
                }
                Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f32)
            }
            FieldType::Int32LE => {
                if bytes.len() < 4 {
                    return None;
                }
                Some((i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])) as f32)
            }
            FieldType::Int32BE => {
                if bytes.len() < 4 {
                    return None;
                }
                Some((i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])) as f32)
            }
            FieldType::Float32LE => {
                if bytes.len() < 4 {
                    return None;
                }
                Some(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            }
            FieldType::Float32BE => {
                if bytes.len() < 4 {
                    return None;
                }
                Some(f32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            }
            FieldType::Bytes => {
                // Bytes 类型输出第一字节 (作为数值预览), 长度由 length_ref 决定
                bytes.first().map(|&b| b as f32)
            }
        }
    }
}

/// 帧解码块的覆盖范围 (校验计算的字节范围)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecoderChecksumCover {
    /// 从帧开头到本校验块之前的所有字节
    AllPrior,
    /// 用户指定字节偏移范围 [cover_start, cover_end)
    Range,
}

/// 帧解码校验位置
/// - Append:  校验字节位于帧末尾 (在 tail 之前)
/// - Inline:  校验字节位于当前位置 (在块列表中该 checksum 块的位置)
/// - Prepend: 校验字节位于帧头之后 (在 header 之后)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecoderChecksumPosition {
    Append,
    Inline,
    Prepend,
}

/// 长度块的单位
/// - Bytes:  字节数 (length 值表示后续字段的字节长度)
/// - Fields: 后续 field 块重复次数 (length 值表示后续 field 块重复 N 次)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LengthUnit {
    Bytes,
    Fields,
}

/// 帧解码块定义 (与前端 DecoderBlock 对应, serde tag="type" + camelCase)
///
/// 使用 `tag = "type"` (无 content) 模式: 每个 variant 的所有字段直接在对象顶层,
/// 与前端 DecoderBlock 结构一致 (id/type/fieldType/portName/... 同级)。
///
/// 每个块都有 `id` 字段 (前端生成的唯一标识, 用于 length_ref 引用)。
/// 每个块可选 `match_id` 字段 (Id 块除外) — 仅当当前帧的 id_value 等于 match_id 时该块执行。
/// 未设置 match_id 的块始终执行 (用于多帧类型分派)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DecoderBlockDef {
    /// 帧头: 匹配固定字节序列 (帧起始标志)
    Header {
        /// 块 id (前端生成, 用于 UI 引用)
        id: String,
        /// HEX 字符串, 如 "AA BB" (空格可选)
        hex: String,
        /// 可选 match_id (用于多帧类型分派)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        match_id: Option<i64>,
    },
    /// 长度字段: 读 N 字节为整数, 输出到 length 端口 + 决定后续变长字段长度
    Length {
        id: String,
        field_type: FieldType,
        /// 输出端口名 (默认 "length")
        #[serde(default, skip_serializing_if = "Option::is_none")]
        port_name: Option<String>,
        /// 长度单位 (默认 bytes)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unit: Option<LengthUnit>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        match_id: Option<i64>,
    },
    /// 帧类型 ID: 读 N 字节为整数, 输出到 id_value 端口 + 设置 match_id 上下文
    Id {
        id: String,
        field_type: FieldType,
        /// 输出端口名 (默认 "id_value")
        #[serde(default, skip_serializing_if = "Option::is_none")]
        port_name: Option<String>,
    },
    /// 数据字段: 按 field_type 读 N 字节并解码为 f32, 输出到 port_name 端口
    Field {
        id: String,
        field_type: FieldType,
        /// 输出端口名 (节点上暴露的 Handle id)
        port_name: String,
        /// 若设置, 引用某个 Length 块的 id — 该字段读取 length_value 字节而非 field_type 固定长度
        /// (仅 field_type=Bytes 时生效, 输出第一字节为 f32; 其他类型忽略此字段)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        length_ref: Option<String>,
        /// 仅当 id_value === match_id 时执行 (多帧分派)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        match_id: Option<i64>,
    },
    /// 位域字段: 从指定字节按 bit 偏移+位长读取, 输出到 port_name 端口
    Bitfield {
        id: String,
        /// 字节偏移 (相对于帧头之后的位置)
        byte_offset: u32,
        /// 位偏移 (0-7)
        bit_offset: u8,
        /// 位长度 (1-32)
        bit_length: u8,
        /// 是否带符号 (true=最高位为符号位, 二补码)
        is_signed: bool,
        /// 输出端口名
        port_name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        match_id: Option<i64>,
    },
    /// 校验: 对前序累计字节校验, 输出 valid 端口 (1.0/0.0)
    Checksum {
        id: String,
        /// 校验算法
        algorithm: ChecksumAlgorithm,
        /// 自定义脚本 (algorithm=Custom 时使用)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        custom_script: Option<String>,
        /// 校验覆盖范围
        cover: DecoderChecksumCover,
        /// cover=Range 时的起始字节偏移 (相对帧头之后)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cover_start: Option<u32>,
        /// cover=Range 时的结束字节偏移 (exclusive)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cover_end: Option<u32>,
        /// 校验字节在帧中的位置
        position: DecoderChecksumPosition,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        match_id: Option<i64>,
    },
    /// 帧尾: 匹配固定字节序列 (可选, 帧结束标志)
    Tail {
        id: String,
        /// HEX 字符串
        hex: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        match_id: Option<i64>,
    },
}

impl DecoderBlockDef {
    /// 返回块的 id
    pub fn id(&self) -> &str {
        match self {
            DecoderBlockDef::Header { id, .. }
            | DecoderBlockDef::Length { id, .. }
            | DecoderBlockDef::Id { id, .. }
            | DecoderBlockDef::Field { id, .. }
            | DecoderBlockDef::Bitfield { id, .. }
            | DecoderBlockDef::Checksum { id, .. }
            | DecoderBlockDef::Tail { id, .. } => id,
        }
    }

    /// 返回该块的 match_id (Id 块返回 None)
    pub fn match_id(&self) -> Option<i64> {
        match self {
            DecoderBlockDef::Header { match_id, .. }
            | DecoderBlockDef::Length { match_id, .. }
            | DecoderBlockDef::Field { match_id, .. }
            | DecoderBlockDef::Bitfield { match_id, .. }
            | DecoderBlockDef::Checksum { match_id, .. }
            | DecoderBlockDef::Tail { match_id, .. } => *match_id,
            DecoderBlockDef::Id { .. } => None,
        }
    }

    /// 返回该块的输出端口名 (有输出端口的块: Length/Id/Field/Bitfield)
    /// Header/Checksum/Tail 无输出端口, 返回 None
    /// Length 默认 "length", Id 默认 "id_value"
    pub fn output_port_name(&self) -> Option<&str> {
        match self {
            DecoderBlockDef::Length { port_name, .. } => {
                Some(port_name.as_deref().unwrap_or("length"))
            }
            DecoderBlockDef::Id { port_name, .. } => {
                Some(port_name.as_deref().unwrap_or("id_value"))
            }
            DecoderBlockDef::Field { port_name, .. } => Some(port_name.as_str()),
            DecoderBlockDef::Bitfield { port_name, .. } => Some(port_name.as_str()),
            DecoderBlockDef::Header { .. }
            | DecoderBlockDef::Checksum { .. }
            | DecoderBlockDef::Tail { .. } => None,
        }
    }
}

/// 节点定义 — 通过 IPC 从前端同步到后端
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeDef {
    pub id: String,
    pub tab_id: String,
    pub kind: NodeKind,
}

/// 回环字节输入端口 handle 名 (FrameDecoder loopback 模式的字节入口)
/// 以该 handle 为 target 的边是"字节路由边", 不参与 f32 拓扑排序/求值
pub const LOOPBACK_IN_HANDLE: &str = "loopbackIn";

/// 取节点的输出 map (不存在则创建) — evaluate_into 热路径用
///
/// 不做 clear: 端口覆盖写, 稳态零分配; 过期端口清理由调用方
/// 在图重编译时清空整个 out 保证。
fn node_out_entry<'a>(out: &'a mut ValuesMap, node_id: &str) -> &'a mut HashMap<String, f32, FxBuildHasher> {
    if out.get_mut(node_id).is_none() {
        out.insert(node_id.to_string(), HashMap::default());
    }
    out.get_mut(node_id).unwrap()
}

/// 写端口值 — 键已存在时原位写 (零分配), 不存在才插入
fn set_port(m: &mut HashMap<String, f32, FxBuildHasher>, port: &str, value: f32) {
    if let Some(slot) = m.get_mut(port) {
        *slot = value;
    } else {
        m.insert(port.to_string(), value);
    }
}

// ============ 编译期槽位评估表 ============

/// 分配一个输出槽位 (同名端口重复分配时后者覆盖索引, 与 set_port 覆盖写语义一致)
fn alloc_slot(
    slot_names: &mut Vec<(String, String)>,
    slot_index: &mut HashMap<(String, String), usize, FxBuildHasher>,
    node_id: &str,
    port: &str,
) -> usize {
    let idx = slot_names.len();
    slot_names.push((node_id.to_string(), port.to_string()));
    slot_index.insert((node_id.to_string(), port.to_string()), idx);
    idx
}

/// 输入边 (node_id, in_name) → 上游输出槽位 (无边/无槽位 = None, 与 resolve_input 缺省 0.0 对应)
fn resolve_slot(
    input_index: &HashMap<String, HashMap<String, (String, String)>>,
    slot_index: &HashMap<(String, String), usize, FxBuildHasher>,
    node_id: &str,
    in_name: &str,
) -> Option<usize> {
    input_index
        .get(node_id)
        .and_then(|ports| ports.get(in_name))
        .and_then(|(sn, sp)| slot_index.get(&(sn.clone(), sp.clone())).copied())
}

/// 编译期槽位操作 — 平坦操作序列 (拓扑序 == eval_order), 逐帧评估零字符串哈希
enum CompiledOp {
    /// ChannelSource: frame.channels[ch] → slot (越界写 0.0, 与 evaluate_into 语义一致)
    Channel { ch: usize, slot: usize },
    /// Input: input_values[node_id] → slot (缺省 0.0)
    Input { node_id: String, slot: usize },
    /// Math: 从输入槽位收集 → op.evaluate → out 槽位 (输入槽位 None = 常量 0.0)
    Math {
        op: MathOp,
        inputs: Vec<Option<usize>>,
        out: usize,
    },
    /// Custom: custom_outputs[node_id][port] → 各 slot (缺省全部 0.0)
    Custom {
        node_id: String,
        ports: Vec<(String, usize)>,
    },
    /// Filter: 读 in 槽位 → filter_states[node_id] (懒建/kind 变更重建, 与现语义一致) → out
    Filter {
        node_id: String,
        kind: FilterKind,
        input: Option<usize>,
        out: usize,
    },
    /// FrameDecoder: decoder_states[node_id].last_frame → 各端口 slot
    /// (端口列表编译期确定: blocks 的 port_name (默认名规则与 output_port_name 一致)
    ///  + 按开关的 valid/frame_count/last_timestamp/fps)
    FrameDecoder {
        node_id: String,
        ports: Vec<(String, usize)>,
        valid: Option<usize>,
        frame_count: Option<usize>,
        last_timestamp: Option<usize>,
        fps: Option<usize>,
    },
}

/// 编译期槽位评估表 — CompiledGraph::compile 时构建, 逐帧评估纯数组读写
pub struct CompiledEval {
    /// 槽位 i 对应的 (node_id, port) — 供快照物化/派生边反查
    slot_names: Vec<(String, String)>,
    /// (node_id, port) → 槽位下标
    slot_index: HashMap<(String, String), usize, FxBuildHasher>,
    /// 平坦操作序列 (拓扑序 == eval_order)
    ops: Vec<CompiledOp>,
    /// SpectrumSink 输入槽位: (sink_node_id, 源值槽位; None = 无上游边, 与缺省 0.0 对应)
    spectrum_slots: Vec<(String, Option<usize>)>,
}

impl CompiledEval {
    /// 编译期构建: 遍历 eval_order 按节点 kind 分配输出槽位 + 生成平坦操作序列
    ///
    /// 输入边在编译期经 input_index 反查 slot_index 解析为槽位下标;
    /// 查不到 = 常量 0.0 (与 resolve_input 缺省语义一致, 以 None 表示)。
    fn build(
        nodes: &HashMap<String, NodeDef>,
        eval_order: &[String],
        input_index: &HashMap<String, HashMap<String, (String, String)>>,
        ch_names: &[String],
        in_names: &[String],
    ) -> Self {
        let mut slot_names: Vec<(String, String)> = Vec::new();
        let mut slot_index: HashMap<(String, String), usize, FxBuildHasher> = HashMap::default();
        let mut ops: Vec<CompiledOp> = Vec::new();

        for node_id in eval_order {
            let Some(node) = nodes.get(node_id) else {
                continue;
            };
            match &node.kind {
                NodeKind::ChannelSource { channels } => {
                    for i in 0..*channels {
                        let port = ch_names
                            .get(i)
                            .cloned()
                            .unwrap_or_else(|| format!("ch{}", i));
                        let slot = alloc_slot(&mut slot_names, &mut slot_index, node_id, &port);
                        ops.push(CompiledOp::Channel { ch: i, slot });
                    }
                }
                NodeKind::Input => {
                    let slot = alloc_slot(&mut slot_names, &mut slot_index, node_id, "value");
                    ops.push(CompiledOp::Input {
                        node_id: node_id.clone(),
                        slot,
                    });
                }
                NodeKind::Math { op, input_count } => {
                    let inputs = (0..*input_count)
                        .map(|i| {
                            let in_name = in_names
                                .get(i)
                                .cloned()
                                .unwrap_or_else(|| format!("in{}", i));
                            resolve_slot(input_index, &slot_index, node_id, &in_name)
                        })
                        .collect();
                    let out = alloc_slot(&mut slot_names, &mut slot_index, node_id, "result");
                    ops.push(CompiledOp::Math {
                        op: *op,
                        inputs,
                        out,
                    });
                }
                NodeKind::Custom { outputs, .. } => {
                    let ports = outputs
                        .iter()
                        .map(|p| {
                            (
                                p.clone(),
                                alloc_slot(&mut slot_names, &mut slot_index, node_id, p),
                            )
                        })
                        .collect();
                    ops.push(CompiledOp::Custom {
                        node_id: node_id.clone(),
                        ports,
                    });
                }
                NodeKind::Filter { kind } => {
                    let input = resolve_slot(input_index, &slot_index, node_id, "in0");
                    let out = alloc_slot(&mut slot_names, &mut slot_index, node_id, "result");
                    ops.push(CompiledOp::Filter {
                        node_id: node_id.clone(),
                        kind: kind.clone(),
                        input,
                        out,
                    });
                }
                NodeKind::FrameDecoder {
                    blocks,
                    enable_valid,
                    enable_frame_count,
                    enable_last_timestamp,
                    enable_fps,
                    ..
                } => {
                    let mut ports = Vec::new();
                    for b in blocks {
                        if let Some(port) = b.output_port_name() {
                            let slot =
                                alloc_slot(&mut slot_names, &mut slot_index, node_id, port);
                            ports.push((port.to_string(), slot));
                        }
                    }
                    let valid = enable_valid
                        .then(|| alloc_slot(&mut slot_names, &mut slot_index, node_id, "valid"));
                    let frame_count = enable_frame_count
                        .then(|| alloc_slot(&mut slot_names, &mut slot_index, node_id, "frame_count"));
                    let last_timestamp = enable_last_timestamp.then(|| {
                        alloc_slot(&mut slot_names, &mut slot_index, node_id, "last_timestamp")
                    });
                    let fps = enable_fps
                        .then(|| alloc_slot(&mut slot_names, &mut slot_index, node_id, "fps"));
                    ops.push(CompiledOp::FrameDecoder {
                        node_id: node_id.clone(),
                        ports,
                        valid,
                        frame_count,
                        last_timestamp,
                        fps,
                    });
                }
                NodeKind::SpectrumSink { .. } | NodeKind::Sink => {
                    // Sink 类节点不应出现在 eval_order 中, 防御性跳过
                    continue;
                }
            }
        }

        // SpectrumSink 输入槽位 (不在 eval_order, 输入端口固定 "in0")
        let mut spectrum_slots = Vec::new();
        for (node_id, node) in nodes {
            if matches!(node.kind, NodeKind::SpectrumSink { .. }) {
                spectrum_slots.push((
                    node_id.clone(),
                    resolve_slot(input_index, &slot_index, node_id, "in0"),
                ));
            }
        }

        Self {
            slot_names,
            slot_index,
            ops,
            spectrum_slots,
        }
    }

    /// 槽位数 (调用方据此分配 slots/written 缓冲并跨帧复用)
    pub fn slot_count(&self) -> usize {
        self.slot_names.len()
    }

    /// (node_id, port) → 槽位 (派生边批首解析用)
    pub fn slot_of(&self, node: &str, port: &str) -> Option<usize> {
        self.slot_index
            .get(&(node.to_string(), port.to_string()))
            .copied()
    }

    /// 逐帧评估: 纯数组读写, 零字符串哈希
    ///
    /// `slots` / `written` 由调用方分配 (长度 == slot_count) 并跨帧复用;
    /// 调用方负责每帧清零 (slots 防上帧值泄漏, written 复刻 "本帧未产出 = 键不存在")。
    /// op 写槽位时置位 written — FrameDecoder 无 parser / Custom 无回传以外的
    /// 缺失都不写 (与 evaluate_into 的 map 语义一致)。
    pub fn run(
        &self,
        frame: &DataFrame,
        input_values: &HashMap<String, f32>,
        custom_outputs: &HashMap<String, HashMap<String, f32>>,
        filter_states: &mut HashMap<String, DigitalFilter>,
        decoder_states: &HashMap<String, FrameParser>,
        slots: &mut [f32],
        written: &mut [bool],
    ) {
        for op in &self.ops {
            match op {
                CompiledOp::Channel { ch, slot } => {
                    slots[*slot] = frame.channels.get(*ch).copied().unwrap_or(0.0);
                    written[*slot] = true;
                }
                CompiledOp::Input { node_id, slot } => {
                    slots[*slot] = input_values.get(node_id).copied().unwrap_or(0.0);
                    written[*slot] = true;
                }
                CompiledOp::Math { op, inputs, out } => {
                    // 16 路以内走栈数组 (与 evaluate_into 一致)
                    let mut stack_buf = [0.0f32; 16];
                    let mut heap_buf;
                    let buf: &mut [f32] = if inputs.len() <= 16 {
                        &mut stack_buf[..inputs.len()]
                    } else {
                        heap_buf = vec![0.0; inputs.len()];
                        &mut heap_buf
                    };
                    for (i, s) in inputs.iter().enumerate() {
                        buf[i] = s.map(|s| slots[s]).unwrap_or(0.0);
                    }
                    slots[*out] = op.evaluate(buf);
                    written[*out] = true;
                }
                CompiledOp::Custom { node_id, ports } => {
                    let vals = custom_outputs.get(node_id);
                    for (port, slot) in ports {
                        slots[*slot] = vals.and_then(|m| m.get(port)).copied().unwrap_or(0.0);
                        written[*slot] = true;
                    }
                }
                CompiledOp::Filter {
                    node_id,
                    kind,
                    input,
                    out,
                } => {
                    let input_val = input.map(|s| slots[s]).unwrap_or(0.0);
                    // 懒初始化 / kind 变化时重建滤波器状态 (与 evaluate_into 一致)
                    let need_rebuild = filter_states
                        .get(node_id)
                        .map(|f| f.kind() != kind)
                        .unwrap_or(true);
                    if need_rebuild {
                        filter_states.insert(node_id.clone(), DigitalFilter::new(kind.clone()));
                    }
                    let filter = filter_states.get_mut(node_id).unwrap();
                    slots[*out] = filter.process(input_val);
                    written[*out] = true;
                }
                CompiledOp::FrameDecoder {
                    node_id,
                    ports,
                    valid,
                    frame_count,
                    last_timestamp,
                    fps,
                } => {
                    if let Some(parser) = decoder_states.get(node_id) {
                        // 仅写 last_frame.outputs 实际包含的端口 (线性扫描, 端口数小)
                        for (k, &v) in &parser.last_frame.outputs {
                            if let Some((_, slot)) = ports.iter().find(|(p, _)| p == k) {
                                slots[*slot] = v;
                                written[*slot] = true;
                            }
                        }
                        if let Some(s) = valid {
                            slots[*s] = if parser.last_frame.valid { 1.0 } else { 0.0 };
                            written[*s] = true;
                        }
                        if let Some(s) = frame_count {
                            slots[*s] = parser.frame_count as f32;
                            written[*s] = true;
                        }
                        if let Some(s) = last_timestamp {
                            slots[*s] = parser.last_frame.timestamp_us as f32;
                            written[*s] = true;
                        }
                        if let Some(s) = fps {
                            slots[*s] = parser.fps();
                            written[*s] = true;
                        }
                    } else {
                        // 节点刚加入但尚未喂入字节: 所有端口默认 0 (与 evaluate_into 一致)
                        for (_, slot) in ports {
                            slots[*slot] = 0.0;
                            written[*slot] = true;
                        }
                        for s in [valid, frame_count, last_timestamp, fps]
                            .into_iter()
                            .flatten()
                        {
                            slots[*s] = 0.0;
                            written[*s] = true;
                        }
                    }
                }
            }
        }
    }

    /// 快照物化: slots + written → ValuesMap (仅快照发布点调用, 非逐帧)
    ///
    /// 只覆盖写本帧已产出的端口, 不清理过期键 (与 evaluate_into 语义一致)
    pub fn materialize(&self, slots: &[f32], written: &[bool], out: &mut ValuesMap) {
        for (i, (node_id, port)) in self.slot_names.iter().enumerate() {
            if written[i] {
                let m = node_out_entry(out, node_id);
                set_port(m, port, slots[i]);
            }
        }
    }

    /// SpectrumSink 输入: (sink_id, value) 迭代, 仅 written 槽位
    pub fn spectrum_values<'a>(
        &'a self,
        slots: &'a [f32],
        written: &'a [bool],
    ) -> impl Iterator<Item = (&'a str, f32)> + 'a {
        self.spectrum_slots.iter().filter_map(move |(sink, slot)| match slot {
            Some(s) if written[*s] => Some((sink.as_str(), slots[*s])),
            _ => None,
        })
    }
}

/// 编译后的图 — 包含拓扑序的评估计划
pub struct CompiledGraph {
    pub tab_id: String,
    /// 所有节点 (含 Sink, 便于前端查询)
    nodes: HashMap<String, NodeDef>,
    /// 边集合
    edges: Vec<Edge>,
    /// 字节路由边 (target_handle == loopbackIn) — 仅用于回环字节注入查找,
    /// 不参与拓扑排序 (避免 Command var_ref 输入回连解码器输出时误判循环)
    byte_edges: Vec<Edge>,
    /// 拓扑序 — 仅包含有输出的节点 (ChannelSource/Input/Math/Custom)
    /// Sink 节点不参与评估
    eval_order: Vec<String>,
    /// 反向索引: target_node → (target_handle → (source_node, source_handle))
    /// 嵌套结构支持 &str 零分配查询 (evaluate_into 热路径)
    input_index: HashMap<String, HashMap<String, (String, String)>>,
    /// ChannelSource 节点 ID (每个 tab 一个)
    channel_source_id: Option<String>,
    /// 编译期缓存: ChannelSource 输出端口名 ch0..chN (避免每帧 format! 分配)
    ch_names: Vec<String>,
    /// 编译期缓存: Math 输入端口名 in0..inN (避免每帧 format! 分配)
    in_names: Vec<String>,
    /// 编译期槽位评估表 (逐帧评估零字符串哈希, process_frames_batch 热路径用)
    compiled: CompiledEval,
}

/// 评估错误
#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    #[error("节点 {0} 不存在于图中")]
    NodeNotFound(String),
    #[error("检测到循环连接")]
    Cycle,
    #[error("通道源节点缺失 (tab_id={0})")]
    NoChannelSource(String),
}

impl CompiledGraph {
    /// 编译图 — 构建拓扑序 + 索引, 检测循环
    pub fn compile(
        tab_id: String,
        nodes: Vec<NodeDef>,
        edges: Vec<Edge>,
    ) -> Result<Self, CompileError> {
        let mut node_map: HashMap<String, NodeDef> = HashMap::new();
        let mut channel_source_id: Option<String> = None;

        for n in nodes {
            if matches!(n.kind, NodeKind::ChannelSource { .. }) {
                channel_source_id = Some(n.id.clone());
            }
            node_map.insert(n.id.clone(), n);
        }

        // 字节路由边 (target_handle == loopbackIn) 单独收集, 不参与 f32 拓扑排序
        // (字节不经 evaluate 流动; 若参与 DFS, Command var_ref 输入回连解码器输出会误判循环)
        let byte_edges: Vec<Edge> = edges
            .iter()
            .filter(|e| e.target_handle == LOOPBACK_IN_HANDLE)
            .cloned()
            .collect();
        let f32_edges: Vec<Edge> = edges
            .iter()
            .filter(|e| e.target_handle != LOOPBACK_IN_HANDLE)
            .cloned()
            .collect();

        // 构建 input_index: target → (target_handle → (source, source_handle))
        // 嵌套结构, 支持 &str 零分配查询
        let mut input_index: HashMap<String, HashMap<String, (String, String)>> = HashMap::new();
        for e in &edges {
            input_index
                .entry(e.target.clone())
                .or_default()
                .insert(
                    e.target_handle.clone(),
                    (e.source.clone(), e.source_handle.clone()),
                );
        }

        // 编译期端口名缓存 (evaluate 热路径避免 format! 分配)
        let ch_count = channel_source_id
            .as_ref()
            .and_then(|id| node_map.get(id))
            .map(|n| match &n.kind {
                NodeKind::ChannelSource { channels } => *channels,
                _ => 0,
            })
            .unwrap_or(0);
        let ch_names: Vec<String> = (0..ch_count).map(|i| format!("ch{}", i)).collect();
        let max_inputs = node_map
            .values()
            .map(|n| match &n.kind {
                NodeKind::Math { input_count, .. } => *input_count,
                _ => 0,
            })
            .max()
            .unwrap_or(0);
        let in_names: Vec<String> = (0..max_inputs).map(|i| format!("in{}", i)).collect();

        // 拓扑排序 — 仅对有输出的节点
        // 使用 DFS 后序
        let mut visited: HashMap<String, u8> = HashMap::new(); // 0=未访问, 1=访问中, 2=已完成
        let mut order: Vec<String> = Vec::new();

        fn dfs(
            id: &str,
            nodes: &HashMap<String, NodeDef>,
            edges: &[Edge],
            visited: &mut HashMap<String, u8>,
            order: &mut Vec<String>,
        ) -> Result<(), CompileError> {
            match visited.get(id) {
                Some(&1) => return Err(CompileError::Cycle),
                Some(&2) => return Ok(()),
                _ => {}
            }
            visited.insert(id.to_string(), 1);

            // 访问上游 (有 edge 指向本节点的源节点)
            for e in edges {
                if e.target == id && nodes.contains_key(&e.source) {
                    dfs(&e.source, nodes, edges, visited, order)?;
                }
            }

            visited.insert(id.to_string(), 2);
            order.push(id.to_string());
            Ok(())
        }

        // 仅对有输出的节点启动 DFS (避免 Sink / SpectrumSink 进入拓扑序)
        // - Sink: 纯消费, 无输出
        // - SpectrumSink: 块运算, 无输出端口, 由独立 30 FPS ticker 触发 FFT
        let output_node_ids: Vec<String> = node_map
            .iter()
            .filter(|(_, n)| !matches!(n.kind, NodeKind::Sink | NodeKind::SpectrumSink { .. }))
            .map(|(id, _)| id.clone())
            .collect();

        for id in &output_node_ids {
            dfs(id, &node_map, &f32_edges, &mut visited, &mut order)?;
        }

        // 编译期槽位评估表 (材料齐备: eval_order/input_index/ch_names/in_names)
        let compiled = CompiledEval::build(&node_map, &order, &input_index, &ch_names, &in_names);

        Ok(Self {
            tab_id,
            nodes: node_map,
            edges,
            byte_edges,
            eval_order: order,
            input_index,
            channel_source_id,
            ch_names,
            in_names,
            compiled,
        })
    }

    /// 评估图 — 给定数据帧 + 输入值 + Custom 回传值 + Filter 状态 + Decoder 状态, 返回所有节点的输出端口值
    ///
    /// 返回: HashMap<widgetId, HashMap<portId, f32>>
    ///   - 包含 ChannelSource/Input/Math/Custom/Filter/FrameDecoder 的输出
    ///   - 不包含 Sink / SpectrumSink (无输出)
    ///
    /// `filter_states`: 滤波器状态 (跨帧持久化), key = Filter 节点 id
    ///   首次遇到 Filter 节点时按其 kind 创建 DigitalFilter 并存入;
    ///   后续帧复用同一状态, 实现逐点滤波的连续性。
    ///   当 Filter 节点的 kind 变化时 (用户修改配置), 自动重建状态。
    ///
    /// `decoder_states`: 帧解码器状态 (跨帧持久化), key = FrameDecoder 节点 id
    ///   由调用方 (data_loop) 通过 feed_frame_decoders 喂入字节流并更新 last_frame。
    ///   evaluate 阶段仅读取 last_frame 缓存的 outputs + 附加端口 (valid/frame_count/last_timestamp/fps)。
    pub fn evaluate(
        &self,
        frame: &DataFrame,
        input_values: &HashMap<String, f32>,
        custom_outputs: &HashMap<String, HashMap<String, f32>>,
        filter_states: &mut HashMap<String, DigitalFilter>,
        decoder_states: &HashMap<String, FrameParser>,
    ) -> ValuesMap {
        let mut out = ValuesMap::default();
        self.evaluate_into(
            frame,
            input_values,
            custom_outputs,
            filter_states,
            decoder_states,
            &mut out,
        );
        out
    }

    /// 评估图 (零分配快路径) — 结果写入调用方提供的 `out`
    ///
    /// 与 evaluate 语义相同, 但稳态下 (图结构不变) 几乎无堆分配:
    /// - `out` 内外层 HashMap 跨帧复用 (调用方每帧传入同一 map, 本函数按节点覆盖写)
    /// - 端口名/输入名用编译期缓存 (ch_names / in_names) 或 &'static str
    /// - input_index 嵌套查询零分配
    ///
    /// 注意: 本函数只覆盖写当前节点的端口, 不清理过期键 — 图结构变化 (重编译)
    /// 时调用方应清空 out (process_frames_batch 通过 graphs_version 检测)。
    pub fn evaluate_into(
        &self,
        frame: &DataFrame,
        input_values: &HashMap<String, f32>,
        custom_outputs: &HashMap<String, HashMap<String, f32>>,
        filter_states: &mut HashMap<String, DigitalFilter>,
        decoder_states: &HashMap<String, FrameParser>,
        out: &mut ValuesMap,
    ) {
        for node_id in &self.eval_order {
            let node = match self.nodes.get(node_id) {
                Some(n) => n,
                None => continue,
            };

            match &node.kind {
                NodeKind::ChannelSource { channels } => {
                    let m = node_out_entry(out, node_id);
                    for i in 0..*channels {
                        let v = frame.channels.get(i).copied().unwrap_or(0.0);
                        set_port(m, &self.ch_names[i], v);
                    }
                }
                NodeKind::Input => {
                    let v = input_values.get(node_id).copied().unwrap_or(0.0);
                    let m = node_out_entry(out, node_id);
                    set_port(m, "value", v);
                }
                NodeKind::Math { op, input_count } => {
                    // 先收集输入 (不可变读 out), 再取可变引用写入;
                    // 16 路以内走栈数组, 避免每帧每节点一次堆分配 (700k 帧/s 下是热路径)
                    let mut stack_buf = [0.0f32; 16];
                    let mut heap_buf;
                    let inputs: &mut [f32] = if *input_count <= 16 {
                        &mut stack_buf[..*input_count]
                    } else {
                        heap_buf = vec![0.0; *input_count];
                        &mut heap_buf
                    };
                    for (i, slot) in inputs.iter_mut().enumerate() {
                        *slot = self.resolve_input(node_id, &self.in_names[i], out);
                    }
                    let result = op.evaluate(inputs);
                    let m = node_out_entry(out, node_id);
                    set_port(m, "result", result);
                }
                NodeKind::Custom { outputs, .. } => {
                    // 输出来自前端回传
                    let m = node_out_entry(out, node_id);
                    if let Some(vals) = custom_outputs.get(node_id) {
                        for (k, &v) in vals {
                            set_port(m, k, v);
                        }
                    } else {
                        // 默认: 所有输出端口为 0
                        for p in outputs {
                            set_port(m, p, 0.0);
                        }
                    }
                }
                NodeKind::Filter { kind } => {
                    // 取输入 "in0" 的上游值
                    let input_val = self.resolve_input(node_id, "in0", out);
                    // 懒初始化 / kind 变化时重建滤波器状态
                    let need_rebuild = filter_states
                        .get(node_id)
                        .map(|f| f.kind() != kind)
                        .unwrap_or(true);
                    if need_rebuild {
                        filter_states.insert(node_id.clone(), DigitalFilter::new(kind.clone()));
                    }
                    let filter = filter_states.get_mut(node_id).unwrap();
                    let result = filter.process(input_val);
                    let m = node_out_entry(out, node_id);
                    set_port(m, "result", result);
                }
                NodeKind::FrameDecoder {
                    blocks,
                    enable_valid,
                    enable_frame_count,
                    enable_last_timestamp,
                    enable_fps,
                    loopback: _,
                } => {
                    // FrameDecoder 的输出由 data_loop 喂入字节流后缓存到 decoder_states,
                    // evaluate 阶段仅读取 last_frame 缓存。
                    // 若 decoder_states 中无此节点 (尚未收到字节), 返回空 outputs + 默认 valid=0。
                    let m = node_out_entry(out, node_id);
                    if let Some(parser) = decoder_states.get(node_id) {
                        for (k, &v) in &parser.last_frame.outputs {
                            set_port(m, k, v);
                        }
                        // 附加输出端口
                        if *enable_valid {
                            set_port(m, "valid", if parser.last_frame.valid { 1.0 } else { 0.0 });
                        }
                        if *enable_frame_count {
                            set_port(m, "frame_count", parser.frame_count as f32);
                        }
                        if *enable_last_timestamp {
                            set_port(m, "last_timestamp", parser.last_frame.timestamp_us as f32);
                        }
                        if *enable_fps {
                            set_port(m, "fps", parser.fps());
                        }
                    } else {
                        // 节点刚加入但尚未喂入字节: 输出所有端口的默认 0
                        for b in blocks {
                            if let Some(port) = b.output_port_name() {
                                set_port(m, port, 0.0);
                            }
                        }
                        if *enable_valid {
                            set_port(m, "valid", 0.0);
                        }
                        if *enable_frame_count {
                            set_port(m, "frame_count", 0.0);
                        }
                        if *enable_last_timestamp {
                            set_port(m, "last_timestamp", 0.0);
                        }
                        if *enable_fps {
                            set_port(m, "fps", 0.0);
                        }
                    }
                }
                NodeKind::SpectrumSink { .. } | NodeKind::Sink => {
                    // Sink 类节点不应出现在 eval_order 中, 防御性跳过
                    continue;
                }
            }
        }
    }

    /// 解析某节点某输入端口的上游输出值
    /// (在 evaluate 过程中, 上游必然已计算完成)
    fn resolve_input(
        &self,
        node_id: &str,
        port_id: &str,
        computed: &ValuesMap,
    ) -> f32 {
        if let Some((src_node, src_port)) = self
            .input_index
            .get(node_id)
            .and_then(|ports| ports.get(port_id))
        {
            computed
                .get(src_node)
                .and_then(|m| m.get(src_port))
                .copied()
                .unwrap_or(0.0)
        } else {
            0.0
        }
    }

    /// 收集所有 Custom 节点的当前输入值 (供推送到前端 iframe)
    /// 返回: HashMap<custom_widget_id, HashMap<input_port_id, value>>
    pub fn collect_custom_inputs(
        &self,
        computed: &ValuesMap,
    ) -> HashMap<String, HashMap<String, f32>> {
        let mut result = HashMap::new();
        for (node_id, node) in &self.nodes {
            if let NodeKind::Custom { inputs, .. } = &node.kind {
                let mut m = HashMap::with_capacity(inputs.len());
                for port in inputs {
                    let val = self.resolve_input(node_id, port, computed);
                    m.insert(port.clone(), val);
                }
                result.insert(node_id.clone(), m);
            }
        }
        result
    }

    /// 收集所有 SpectrumSink 节点的当前输入值 (供 data_loop 推入频谱分析器)
    ///
    /// SpectrumSink 的输入端口固定为 "in0", 取上游输出值。
    /// 返回: HashMap<sink_widget_id, input_value>
    /// 调用方 (data_loop) 在每帧 evaluate 后调用本方法,
    /// 将值 push 到对应的 SpectrumAnalyzer 的滑动窗口。
    pub fn collect_spectrum_inputs(
        &self,
        computed: &ValuesMap,
    ) -> HashMap<String, f32> {
        let mut result = HashMap::new();
        for (node_id, node) in &self.nodes {
            if matches!(node.kind, NodeKind::SpectrumSink { .. }) {
                let val = self.resolve_input(node_id, "in0", computed);
                result.insert(node_id.clone(), val);
            }
        }
        result
    }

    /// 获取所有 Custom 节点 id
    pub fn custom_node_ids(&self) -> Vec<String> {
        self.nodes
            .iter()
            .filter(|(_, n)| matches!(n.kind, NodeKind::Custom { .. }))
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// 获取所有 SpectrumSink 节点 id
    pub fn spectrum_sink_ids(&self) -> Vec<String> {
        self.nodes
            .iter()
            .filter(|(_, n)| matches!(n.kind, NodeKind::SpectrumSink { .. }))
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// 获取所有 Filter 节点 id (供状态清理: 删除节点时移除对应 filter_states)
    pub fn filter_node_ids(&self) -> Vec<String> {
        self.nodes
            .iter()
            .filter(|(_, n)| matches!(n.kind, NodeKind::Filter { .. }))
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// 获取所有 FrameDecoder 节点 id
    /// (供 data_loop 同步 decoder_states: 创建/重建/清理 FrameParser)
    pub fn decoder_node_ids(&self) -> Vec<String> {
        self.nodes
            .iter()
            .filter(|(_, n)| matches!(n.kind, NodeKind::FrameDecoder { .. }))
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// 获取 FrameDecoder 节点的配置 (blocks + 附加端口开关 + loopback 标志)
    /// 用于 decoder_feed / inject_loopback_bytes 在节点变更时重建 FrameParser
    pub fn decoder_config(
        &self,
        node_id: &str,
    ) -> Option<(&[DecoderBlockDef], bool, bool, bool, bool, bool)> {
        let node = self.nodes.get(node_id)?;
        if let NodeKind::FrameDecoder {
            blocks,
            enable_valid,
            enable_frame_count,
            enable_last_timestamp,
            enable_fps,
            loopback,
        } = &node.kind
        {
            Some((
                blocks.as_slice(),
                *enable_valid,
                *enable_frame_count,
                *enable_last_timestamp,
                *enable_fps,
                *loopback,
            ))
        } else {
            None
        }
    }

    /// 查找回环字节注入的目标解码器:
    /// 字节路由边 (target_handle == loopbackIn) 中, source 为指定控件的所有 FrameDecoder target
    pub fn loopback_targets_for(&self, source_id: &str) -> Vec<String> {
        self.byte_edges
            .iter()
            .filter(|e| {
                e.source == source_id
                    && matches!(
                        self.nodes.get(&e.target).map(|n| &n.kind),
                        Some(NodeKind::FrameDecoder { .. })
                    )
            })
            .map(|e| e.target.clone())
            .collect()
    }

    /// 获取 SpectrumSink 节点的配置 (window_size, window_type, output, sample_rate)
    /// 用于 state.rs 在节点变更时重建 SpectrumAnalyzer
    pub fn spectrum_sink_config(
        &self,
        node_id: &str,
    ) -> Option<(usize, WindowType, SpectrumOutput, f32)> {
        let node = self.nodes.get(node_id)?;
        if let NodeKind::SpectrumSink {
            window_size,
            window_type,
            output,
            sample_rate,
        } = &node.kind
        {
            Some((*window_size, *window_type, *output, *sample_rate))
        } else {
            None
        }
    }

    pub fn nodes(&self) -> &HashMap<String, NodeDef> {
        &self.nodes
    }

    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    pub fn channel_source_id(&self) -> Option<&str> {
        self.channel_source_id.as_deref()
    }

    /// 编译期槽位评估表 (process_frames_batch 热路径用)
    pub fn compiled(&self) -> &CompiledEval {
        &self.compiled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vofa_next_buffer::graph::Edge;

    fn make_channel_source(tab_id: &str, channels: usize) -> NodeDef {
        NodeDef {
            id: format!("__channel_source__-{}", tab_id),
            tab_id: tab_id.to_string(),
            kind: NodeKind::ChannelSource { channels },
        }
    }

    fn make_math(id: &str, tab_id: &str, op: MathOp, input_count: usize) -> NodeDef {
        NodeDef {
            id: id.to_string(),
            tab_id: tab_id.to_string(),
            kind: NodeKind::Math { op, input_count },
        }
    }

    fn make_input(id: &str, tab_id: &str) -> NodeDef {
        NodeDef {
            id: id.to_string(),
            tab_id: tab_id.to_string(),
            kind: NodeKind::Input,
        }
    }

    fn make_sink(id: &str, tab_id: &str) -> NodeDef {
        NodeDef {
            id: id.to_string(),
            tab_id: tab_id.to_string(),
            kind: NodeKind::Sink,
        }
    }

    fn make_custom(id: &str, tab_id: &str, inputs: Vec<&str>, outputs: Vec<&str>) -> NodeDef {
        NodeDef {
            id: id.to_string(),
            tab_id: tab_id.to_string(),
            kind: NodeKind::Custom {
                inputs: inputs.iter().map(|s| s.to_string()).collect(),
                outputs: outputs.iter().map(|s| s.to_string()).collect(),
            },
        }
    }

    fn make_filter(id: &str, tab_id: &str, kind: FilterKind) -> NodeDef {
        NodeDef {
            id: id.to_string(),
            tab_id: tab_id.to_string(),
            kind: NodeKind::Filter { kind },
        }
    }

    fn make_spectrum_sink(
        id: &str,
        tab_id: &str,
        window_size: usize,
        window_type: WindowType,
        output: SpectrumOutput,
        sample_rate: f32,
    ) -> NodeDef {
        NodeDef {
            id: id.to_string(),
            tab_id: tab_id.to_string(),
            kind: NodeKind::SpectrumSink {
                window_size,
                window_type,
                output,
                sample_rate,
            },
        }
    }

    fn edge(id: &str, src: &str, src_h: &str, tgt: &str, tgt_h: &str) -> Edge {
        Edge {
            id: id.to_string(),
            source: src.to_string(),
            source_handle: src_h.to_string(),
            target: tgt.to_string(),
            target_handle: tgt_h.to_string(),
        }
    }

    #[test]
    fn test_compile_empty() {
        let g = CompiledGraph::compile("t1".into(), vec![], vec![]).unwrap();
        assert!(g.eval_order.is_empty());
    }

    #[test]
    fn test_cycle_detection() {
        let nodes = vec![
            make_math("a", "t1", MathOp::Add, 1),
            make_math("b", "t1", MathOp::Add, 1),
        ];
        let edges = vec![
            edge("e1", "a", "result", "b", "in0"),
            edge("e2", "b", "result", "a", "in0"),
        ];
        let result = CompiledGraph::compile("t1".into(), nodes, edges);
        assert!(matches!(result, Err(CompileError::Cycle)));
    }

    #[test]
    fn test_evaluate_channel_source() {
        let nodes = vec![make_channel_source("t1", 2)];
        let g = CompiledGraph::compile("t1".into(), nodes, vec![]).unwrap();
        let frame = DataFrame::new(vec![10.0, 20.0]);
        let input_values = HashMap::new();
        let custom_outputs = HashMap::new();
        let mut filter_states = HashMap::new();
        let out = g.evaluate(
            &frame,
            &input_values,
            &custom_outputs,
            &mut filter_states,
            &HashMap::new(),
        );
        let cs_id = "__channel_source__-t1";
        assert_eq!(out.get(cs_id).and_then(|m| m.get("ch0")), Some(&10.0));
        assert_eq!(out.get(cs_id).and_then(|m| m.get("ch1")), Some(&20.0));
    }

    #[test]
    fn test_evaluate_input_node() {
        let nodes = vec![make_input("knob1", "t1")];
        let g = CompiledGraph::compile("t1".into(), nodes, vec![]).unwrap();
        let frame = DataFrame::new(vec![]);
        let mut input_values = HashMap::new();
        input_values.insert("knob1".to_string(), 42.0_f32);
        let custom_outputs = HashMap::new();
        let mut filter_states = HashMap::new();
        let out = g.evaluate(
            &frame,
            &input_values,
            &custom_outputs,
            &mut filter_states,
            &HashMap::new(),
        );
        assert_eq!(out.get("knob1").and_then(|m| m.get("value")), Some(&42.0));
    }

    #[test]
    fn test_evaluate_math_add() {
        let nodes = vec![
            make_channel_source("t1", 2),
            make_math("m1", "t1", MathOp::Add, 2),
        ];
        let edges = vec![
            edge("e1", "__channel_source__-t1", "ch0", "m1", "in0"),
            edge("e2", "__channel_source__-t1", "ch1", "m1", "in1"),
        ];
        let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
        let frame = DataFrame::new(vec![10.0, 20.0]);
        let input_values = HashMap::new();
        let custom_outputs = HashMap::new();
        let mut filter_states = HashMap::new();
        let out = g.evaluate(
            &frame,
            &input_values,
            &custom_outputs,
            &mut filter_states,
            &HashMap::new(),
        );
        // m1.result = 10 + 20 = 30
        assert_eq!(out.get("m1").and_then(|m| m.get("result")), Some(&30.0));
    }

    #[test]
    fn test_evaluate_math_chain() {
        // m1 = ch0 + ch1, m2 = m1 * 2
        let nodes = vec![
            make_channel_source("t1", 2),
            make_math("m1", "t1", MathOp::Add, 2),
            make_math("m2", "t1", MathOp::Mul, 2),
        ];
        let edges = vec![
            edge("e1", "__channel_source__-t1", "ch0", "m1", "in0"),
            edge("e2", "__channel_source__-t1", "ch1", "m1", "in1"),
            edge("e3", "m1", "result", "m2", "in0"),
            edge("e4", "m1", "result", "m2", "in1"), // m2 = m1 * m1
        ];
        let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
        let frame = DataFrame::new(vec![3.0, 4.0]);
        let input_values = HashMap::new();
        let custom_outputs = HashMap::new();
        let mut filter_states = HashMap::new();
        let out = g.evaluate(
            &frame,
            &input_values,
            &custom_outputs,
            &mut filter_states,
            &HashMap::new(),
        );
        // m1 = 3 + 4 = 7, m2 = 7 * 7 = 49
        assert_eq!(out.get("m1").and_then(|m| m.get("result")), Some(&7.0));
        assert_eq!(out.get("m2").and_then(|m| m.get("result")), Some(&49.0));
    }

    #[test]
    fn test_evaluate_custom_node() {
        let nodes = vec![
            make_channel_source("t1", 1),
            make_custom("c1", "t1", vec!["value"], vec!["out"]),
        ];
        let edges = vec![edge("e1", "__channel_source__-t1", "ch0", "c1", "value")];
        let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
        let frame = DataFrame::new(vec![5.0]);
        let input_values = HashMap::new();
        let mut custom_outputs: HashMap<String, HashMap<String, f32>> = HashMap::new();
        let mut m = HashMap::new();
        m.insert("out".to_string(), 99.0);
        custom_outputs.insert("c1".to_string(), m);

        let mut filter_states = HashMap::new();
        let out = g.evaluate(
            &frame,
            &input_values,
            &custom_outputs,
            &mut filter_states,
            &HashMap::new(),
        );
        assert_eq!(out.get("c1").and_then(|m| m.get("out")), Some(&99.0));

        // collect_custom_inputs 应返回 c1.value = 5.0
        let custom_inputs = g.collect_custom_inputs(&out);
        assert_eq!(
            custom_inputs.get("c1").and_then(|m| m.get("value")),
            Some(&5.0)
        );
    }

    #[test]
    fn test_sink_not_in_eval_order() {
        let nodes = vec![make_channel_source("t1", 1), make_sink("gauge1", "t1")];
        let edges = vec![edge(
            "e1",
            "__channel_source__-t1",
            "ch0",
            "gauge1",
            "value",
        )];
        let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
        // Sink 不应在 eval_order 中
        assert!(!g.eval_order.contains(&"gauge1".to_string()));
        // ChannelSource 应在 eval_order 中
        assert!(g.eval_order.contains(&"__channel_source__-t1".to_string()));
    }

    #[test]
    fn test_unary_math() {
        let nodes = vec![
            make_channel_source("t1", 1),
            make_math("m1", "t1", MathOp::Abs, 1),
        ];
        let edges = vec![edge("e1", "__channel_source__-t1", "ch0", "m1", "in0")];
        let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
        let frame = DataFrame::new(vec![-5.0]);
        let input_values = HashMap::new();
        let custom_outputs = HashMap::new();
        let mut filter_states = HashMap::new();
        let out = g.evaluate(
            &frame,
            &input_values,
            &custom_outputs,
            &mut filter_states,
            &HashMap::new(),
        );
        assert_eq!(out.get("m1").and_then(|m| m.get("result")), Some(&5.0));
    }

    // ============ Filter 节点测试 ============

    #[test]
    fn test_filter_fir_passthrough() {
        // FIR b=[1.0] → 通过 (y = x)
        let nodes = vec![
            make_channel_source("t1", 1),
            make_filter("f1", "t1", FilterKind::FIR { b: vec![1.0] }),
        ];
        let edges = vec![edge("e1", "__channel_source__-t1", "ch0", "f1", "in0")];
        let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
        let frame = DataFrame::new(vec![7.5]);
        let input_values = HashMap::new();
        let custom_outputs = HashMap::new();
        let mut filter_states = HashMap::new();
        let out = g.evaluate(
            &frame,
            &input_values,
            &custom_outputs,
            &mut filter_states,
            &HashMap::new(),
        );
        assert_eq!(out.get("f1").and_then(|m| m.get("result")), Some(&7.5));
        // filter_states 应包含 f1
        assert!(filter_states.contains_key("f1"));
    }

    #[test]
    fn test_filter_fir_delay_state_persistence() {
        // FIR b=[0.0, 1.0] → 延迟一拍 (y[n] = x[n-1])
        // 验证 filter_states 跨帧持久化
        let nodes = vec![
            make_channel_source("t1", 1),
            make_filter("f1", "t1", FilterKind::FIR { b: vec![0.0, 1.0] }),
        ];
        let edges = vec![edge("e1", "__channel_source__-t1", "ch0", "f1", "in0")];
        let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
        let input_values = HashMap::new();
        let custom_outputs = HashMap::new();
        let mut filter_states = HashMap::new();

        // 帧 1: x=1.0, y=0.0 (x[-1]=0)
        let out1 = g.evaluate(
            &DataFrame::new(vec![1.0]),
            &input_values,
            &custom_outputs,
            &mut filter_states,
            &HashMap::new(),
        );
        assert_eq!(out1.get("f1").and_then(|m| m.get("result")), Some(&0.0));

        // 帧 2: x=2.0, y=1.0 (x[0]=1, 状态持久化生效)
        let out2 = g.evaluate(
            &DataFrame::new(vec![2.0]),
            &input_values,
            &custom_outputs,
            &mut filter_states,
            &HashMap::new(),
        );
        assert_eq!(out2.get("f1").and_then(|m| m.get("result")), Some(&1.0));

        // 帧 3: x=3.0, y=2.0
        let out3 = g.evaluate(
            &DataFrame::new(vec![3.0]),
            &input_values,
            &custom_outputs,
            &mut filter_states,
            &HashMap::new(),
        );
        assert_eq!(out3.get("f1").and_then(|m| m.get("result")), Some(&2.0));
    }

    #[test]
    fn test_filter_kind_change_rebuilds_state() {
        // 用户修改 Filter 配置时, 状态应重建
        // 初始: FIR b=[1.0] (通过)
        let nodes = vec![
            make_channel_source("t1", 1),
            make_filter("f1", "t1", FilterKind::FIR { b: vec![1.0] }),
        ];
        let edges = vec![edge("e1", "__channel_source__-t1", "ch0", "f1", "in0")];
        let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
        let input_values = HashMap::new();
        let custom_outputs = HashMap::new();
        let mut filter_states = HashMap::new();

        // 帧 1: 通过, y=5.0
        let _ = g.evaluate(
            &DataFrame::new(vec![5.0]),
            &input_values,
            &custom_outputs,
            &mut filter_states,
            &HashMap::new(),
        );
        assert!(filter_states.contains_key("f1"));

        // 重新编译图: 修改 Filter kind 为 b=[2.0] (放大 2 倍)
        let nodes2 = vec![
            make_channel_source("t1", 1),
            make_filter("f1", "t1", FilterKind::FIR { b: vec![2.0] }),
        ];
        let edges2 = vec![edge("e1", "__channel_source__-t1", "ch0", "f1", "in0")];
        let g2 = CompiledGraph::compile("t1".into(), nodes2, edges2).unwrap();
        // 帧 2: 新 kind, 应重建状态, y = 2.0 * 3.0 = 6.0
        let out2 = g2.evaluate(
            &DataFrame::new(vec![3.0]),
            &input_values,
            &custom_outputs,
            &mut filter_states,
            &HashMap::new(),
        );
        assert_eq!(out2.get("f1").and_then(|m| m.get("result")), Some(&6.0));
    }

    #[test]
    fn test_filter_lowpass_preserves_dc() {
        // 低通滤波器对直流信号 (常数) 应基本保持原值
        let nodes = vec![
            make_channel_source("t1", 1),
            make_filter(
                "f1",
                "t1",
                FilterKind::IIR {
                    b: vofa_next_dsp::filter::lowpass_biquad(100.0, 1000.0).0,
                    a: vofa_next_dsp::filter::lowpass_biquad(100.0, 1000.0).1,
                },
            ),
        ];
        let edges = vec![edge("e1", "__channel_source__-t1", "ch0", "f1", "in0")];
        let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
        let input_values = HashMap::new();
        let custom_outputs = HashMap::new();
        let mut filter_states = HashMap::new();

        // 连续输入 1.0 (直流), 稳态后应接近 1.0
        let mut last_y = 0.0;
        for _ in 0..200 {
            let out = g.evaluate(
                &DataFrame::new(vec![1.0]),
                &input_values,
                &custom_outputs,
                &mut filter_states,
                &HashMap::new(),
            );
            last_y = out
                .get("f1")
                .and_then(|m| m.get("result"))
                .copied()
                .unwrap_or(0.0);
        }
        assert!(
            (last_y - 1.0).abs() < 0.01,
            "低通滤波器直流稳态应接近 1.0, 实际 {}",
            last_y
        );
    }

    #[test]
    fn test_filter_in_eval_order() {
        // Filter 应在 eval_order 中 (有输出)
        let nodes = vec![
            make_channel_source("t1", 1),
            make_filter("f1", "t1", FilterKind::FIR { b: vec![1.0] }),
        ];
        let edges = vec![edge("e1", "__channel_source__-t1", "ch0", "f1", "in0")];
        let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
        assert!(g.eval_order.contains(&"f1".to_string()));
        assert!(g.filter_node_ids().contains(&"f1".to_string()));
    }

    // ============ SpectrumSink 节点测试 ============

    #[test]
    fn test_spectrum_sink_not_in_eval_order() {
        // SpectrumSink 不应在 eval_order 中 (无输出, 块运算)
        let nodes = vec![
            make_channel_source("t1", 1),
            make_spectrum_sink(
                "s1",
                "t1",
                256,
                WindowType::Hann,
                SpectrumOutput::Magnitude,
                1000.0,
            ),
        ];
        let edges = vec![edge("e1", "__channel_source__-t1", "ch0", "s1", "in0")];
        let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
        assert!(!g.eval_order.contains(&"s1".to_string()));
        assert!(g.eval_order.contains(&"__channel_source__-t1".to_string()));
        assert!(g.spectrum_sink_ids().contains(&"s1".to_string()));
    }

    #[test]
    fn test_collect_spectrum_inputs() {
        // collect_spectrum_inputs 应返回 SpectrumSink 的输入值
        let nodes = vec![
            make_channel_source("t1", 1),
            make_spectrum_sink(
                "s1",
                "t1",
                256,
                WindowType::Hann,
                SpectrumOutput::Magnitude,
                1000.0,
            ),
        ];
        let edges = vec![edge("e1", "__channel_source__-t1", "ch0", "s1", "in0")];
        let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
        let frame = DataFrame::new(vec![42.0]);
        let input_values = HashMap::new();
        let custom_outputs = HashMap::new();
        let mut filter_states = HashMap::new();
        let out = g.evaluate(
            &frame,
            &input_values,
            &custom_outputs,
            &mut filter_states,
            &HashMap::new(),
        );

        // collect_spectrum_inputs 应返回 s1 → 42.0
        let spectrum_inputs = g.collect_spectrum_inputs(&out);
        assert_eq!(spectrum_inputs.get("s1"), Some(&42.0));
    }

    #[test]
    fn test_spectrum_sink_config() {
        let nodes = vec![
            make_channel_source("t1", 1),
            make_spectrum_sink(
                "s1",
                "t1",
                512,
                WindowType::Blackman,
                SpectrumOutput::PSD,
                2000.0,
            ),
        ];
        let g = CompiledGraph::compile("t1".into(), nodes, vec![]).unwrap();
        let cfg = g.spectrum_sink_config("s1").expect("应能获取配置");
        assert_eq!(cfg.0, 512); // window_size
        assert_eq!(cfg.1, WindowType::Blackman); // window_type
        assert_eq!(cfg.2, SpectrumOutput::PSD); // output
        assert!((cfg.3 - 2000.0).abs() < 1e-6); // sample_rate

        // 不存在的节点应返回 None
        assert!(g.spectrum_sink_config("nonexistent").is_none());
    }

    #[test]
    fn test_spectrum_sink_no_output_in_evaluate() {
        // evaluate 不应包含 SpectrumSink 的输出
        let nodes = vec![
            make_channel_source("t1", 1),
            make_spectrum_sink(
                "s1",
                "t1",
                256,
                WindowType::Hann,
                SpectrumOutput::Magnitude,
                1000.0,
            ),
        ];
        let edges = vec![edge("e1", "__channel_source__-t1", "ch0", "s1", "in0")];
        let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();
        let frame = DataFrame::new(vec![1.0]);
        let input_values = HashMap::new();
        let custom_outputs = HashMap::new();
        let mut filter_states = HashMap::new();
        let out = g.evaluate(
            &frame,
            &input_values,
            &custom_outputs,
            &mut filter_states,
            &HashMap::new(),
        );
        // s1 不应在 evaluate 输出中
        assert!(!out.contains_key("s1"));
        // 但 ChannelSource 应在
        assert!(out.contains_key("__channel_source__-t1"));
    }

    // ============ CompiledEval 槽位评估等价性测试 ============

    /// 槽位评估 (compiled.run + materialize) 与 evaluate_into 逐帧完全等价
    ///
    /// 覆盖: ChannelSource(4ch) / 链式 Math×2 / Filter(FIR, 跨帧状态) / Input /
    /// FrameDecoder 无 parser (默认 0 端口) — 100 帧伪随机数据逐帧比对
    #[test]
    fn test_compiled_eval_equivalence() {
        let cs = "__channel_source__-t1";
        let nodes = vec![
            make_channel_source("t1", 4),
            make_math("m1", "t1", MathOp::Add, 2),
            make_math("m2", "t1", MathOp::Mul, 2),
            make_filter("f1", "t1", FilterKind::FIR { b: vec![0.5, 0.5] }),
            make_input("knob1", "t1"),
            // FrameDecoder 无 parser (decoder_states 为空) — 覆盖 written 语义
            NodeDef {
                id: "d1".to_string(),
                tab_id: "t1".to_string(),
                kind: NodeKind::FrameDecoder {
                    blocks: vec![DecoderBlockDef::Field {
                        id: "f".to_string(),
                        field_type: FieldType::UInt8,
                        port_name: "value".to_string(),
                        length_ref: None,
                        match_id: None,
                    }],
                    enable_valid: true,
                    enable_frame_count: true,
                    enable_last_timestamp: false,
                    enable_fps: false,
                    loopback: false,
                },
            },
        ];
        let edges = vec![
            edge("e1", cs, "ch0", "m1", "in0"),
            edge("e2", cs, "ch1", "m1", "in1"),
            edge("e3", "m1", "result", "m2", "in0"),
            edge("e4", cs, "ch2", "m2", "in1"),
            edge("e5", "m2", "result", "f1", "in0"),
        ];
        let g = CompiledGraph::compile("t1".into(), nodes, edges).unwrap();

        let mut input_values = HashMap::new();
        input_values.insert("knob1".to_string(), 7.0_f32);
        let custom_outputs = HashMap::new();
        let decoder_states = HashMap::new(); // d1 无 parser
        // 两条路径各自独立的 filter_states (跨帧状态)
        let mut fs_a = HashMap::new();
        let mut fs_b = HashMap::new();

        let compiled = g.compiled();
        let n = compiled.slot_count();
        let mut slots = vec![0.0f32; n];
        let mut written = vec![false; n];

        // 确定性伪随机 (LCG)
        let mut seed = 0x12345678u32;
        let mut next_f = move || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 8) as f32 / 16777216.0 * 20.0 - 10.0
        };

        for frame_idx in 0..100 {
            let frame = DataFrame::new(vec![next_f(), next_f(), next_f(), next_f()]);
            // 老路径: evaluate_into
            let mut out_a = ValuesMap::default();
            g.evaluate_into(
                &frame,
                &input_values,
                &custom_outputs,
                &mut fs_a,
                &decoder_states,
                &mut out_a,
            );
            // 新路径: compiled.run + materialize (每帧清零 slots/written)
            slots.fill(0.0);
            written.fill(false);
            compiled.run(
                &frame,
                &input_values,
                &custom_outputs,
                &mut fs_b,
                &decoder_states,
                &mut slots,
                &mut written,
            );
            let mut out_b = ValuesMap::default();
            compiled.materialize(&slots, &written, &mut out_b);
            assert_eq!(out_a, out_b, "帧 {} 输出不一致", frame_idx);
        }

        // FrameDecoder 无 parser: 两边都输出默认 0 端口 (value/valid/frame_count)
        let mut out_a = ValuesMap::default();
        g.evaluate_into(
            &DataFrame::new(vec![1.0, 2.0, 3.0, 4.0]),
            &input_values,
            &custom_outputs,
            &mut fs_a,
            &decoder_states,
            &mut out_a,
        );
        let d1 = out_a.get("d1").expect("d1 应输出默认端口");
        assert_eq!(d1.get("value"), Some(&0.0));
        assert_eq!(d1.get("valid"), Some(&0.0));
        assert_eq!(d1.get("frame_count"), Some(&0.0));
        assert!(!d1.contains_key("last_timestamp")); // enable_last_timestamp = false
    }
}
