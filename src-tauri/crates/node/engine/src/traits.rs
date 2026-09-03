//! 图编译引擎的共用范式 trait — 抽象 NodeSpec / Compilable / Evaluable.
//!
//! 该 trait 模块作为**形状文档**与**调用方 trait 边界**使用：实际编译/求值仍走
//! `CompiledGraph::compile` 与 `CompiledGraph::evaluate_into` 的 inherent 方法。
//!
//! 设计动机（按规划）：
//! - 端口描述结构与 `kind::port_domain` 同源，但通过 trait 表达抽象；
//! - 跨 crate 调用方按 `Compilable<Output = CompiledGraph>` 写约束而不是写具体类型；
//! - 不在 trait 里虚拟掉实际流水线，避免 trait 调用成本与生命周期复杂度。

#![allow(dead_code)] // 形状模块 — trait 由下游 crate 按需启用，未启用前保留为抽象

use hir::CompileError;

/// 端口描述 — trait 形状
#[derive(Debug, Clone)]
pub struct PortDescriptor<'a> {
    pub name: &'a str,
    pub kind: PortKind,
    pub is_output: bool,
}

/// 端口类别
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortKind {
    F32,
    Bytes,
    String,
    /// RawData 关联通道 — `src:<source>:<handle>` 动态端口标记
    RawData {
        source: String,
        handle: String,
    },
}

/// 节点规格 trait — `NodeDef` 是事实上的实现
pub trait NodeSpec {
    fn id(&self) -> &str;
    fn tab_id(&self) -> &str;
    fn outputs(&self) -> Vec<PortDescriptor<'_>>;
    fn inputs(&self) -> Vec<PortDescriptor<'_>>;
}

/// 编译输入 — `CompiledGraph::compile(tab_id, nodes, edges)` 的形状镜像
pub struct CompileInput {
    pub tab_id: String,
    pub nodes: Vec<kind::NodeDef>,
    pub edges: Vec<buffer_graph::Edge>,
}

/// 编译产出 trait
pub trait CompileOutput {}

/// 编译 trait — 入口门面
pub trait Compilable {
    type Output: CompileOutput;
    fn compile(&self, input: CompileInput) -> Result<Self::Output, CompileError>;
}

/// 求值输入 — 各类运行期缓冲与状态表
pub struct EvalInput<'a> {
    pub source_frames: &'a eval::SourceFramesMap,
    pub source_texts: &'a eval::SourceTextsMap,
    pub input_values: &'a std::collections::HashMap<String, f32>,
    pub custom_outputs:
        &'a std::collections::HashMap<String, std::collections::HashMap<String, f32>>,
    pub filter_states: &'a mut std::collections::HashMap<String, dsp_filter::DigitalFilter>,
    pub decoder_states: &'a std::collections::HashMap<String, frame_decoder::FrameParser>,
    pub ifft_states: &'a mut std::collections::HashMap<String, dsp_fft::IfftState>,
    pub trigger_states: &'a mut std::collections::HashMap<String, trigger::TriggerState>,
}

/// 求值输出 — 物化到调用方缓冲
pub struct EvalOutput<'a> {
    pub values: &'a mut eval::ValuesMap,
    pub string_values: &'a mut eval::StringValuesMap,
}

/// 求值 trait
pub trait Evaluable {
    fn evaluate_into(&self, input: EvalInput<'_>) -> EvalOutput<'_>;
}

impl CompileOutput for crate::compile::CompiledGraph {}

// 注：`Compilable` / `Evaluable` 的 blanket impl 留空，由调用方按需在自己 crate 实现，
// 避免 trait 方法隐藏 inherent 的虚拟调用开销。
