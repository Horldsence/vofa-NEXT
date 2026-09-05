//! 编译期槽位操作 (CompiledOp) — 后端产物定义
//!
//! 平坦操作序列 (拓扑序 == 值平面 eval_order), 逐帧评估零字符串哈希。
//! 定义与执行分离: 本模块只定义操作集, 构建见 `lower` 模块,
//! 执行见 [`crate::eval::CompiledEval::run`]。

use dsp_filter::FilterKind;
use kind::{MathOp, NewlineMode, StrOp};
use trigger::TriggerRuleDef;

/// 编译期槽位操作 — 平坦操作序列 (拓扑序 == eval_order), 逐帧评估零字符串哈希
pub enum CompiledOp {
    /// TextOut: 上游字符串槽位 → 本节点 "text" 字符串槽位 (透传写, 供通用发布;
    /// input = None 表示未连接, 不写槽位 → 不触发发送)
    TextOut { input: Option<usize>, out: usize },
    /// ProtocolSource: source_frames[frame_sources[src]].channels[ch] → slot
    /// (源缺失/通道越界写 0.0, 与未连接语义一致)
    ProtocolSource { src: usize, ch: usize, slot: usize },
    /// ProtocolSource 的 "str" 端口 (String 域, RawData 原始字节文本):
    /// source_texts[frame_sources[src]] → 字符串槽位; 源无缓存时不写
    /// (str_written 不置位 → 快照保持上次值, 对齐 Trigger 未激活帧语义)
    ProtocolSourceStr { src: usize, slot: usize },
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
    /// Filter: 读 in 槽位 → filter_states[node_id] (懒建/config 变更重建) → out
    ///
    /// config 透传到运行时, 每帧经 `filter_kind_from_config` 派生 FilterKind;
    /// 比较 config 决定是否重建滤波器状态 (与原 kind 字段语义一致)。
    Filter {
        node_id: String,
        config: FilterKind,
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
    /// Ifft: 读 ifft_states[node_id] 的下一个重建采样 → out 槽位 (环形播放, 时域)
    Ifft { node_id: String, out: usize },
    /// Str: 按 StrOp::input_ports() 端口表紧凑拆分输入 (只含同 domain 端口, 按端口表顺序):
    /// - str_inputs[i] = 第 i 个 String 端口的上游字符串槽位 (None = 未连接/上游无槽位 → str_defaults[i])
    /// - str_defaults[i] = 第 i 个 String 端口的内联回退文本 (编译期捕获; 仅 FORMAT 的
    ///   "fmt" 端口取 NodeKind::Str.tmpl, 其余为空串)
    /// - num_inputs[i] = 第 i 个 F32 端口的上游数值槽位 (None → num_defaults[i])
    /// - num_defaults[i] = 第 i 个 F32 端口的内联回退值 (编译期从 StrNumParams 捕获)
    ///
    /// 输出按 StrOp::output_domain(): String → text_out 字符串槽位, F32 → num_out 数值槽位
    Str {
        op: StrOp,
        str_inputs: Vec<Option<usize>>,
        str_defaults: Box<[String]>,
        num_inputs: Vec<Option<usize>>,
        num_defaults: Vec<f32>,
        text_out: Option<usize>,
        num_out: Option<usize>,
    },
    /// Trigger: 经 trigger_states[node_id] 求值 (懒建 / 配置变更重建, 与 evaluate_into 一致)
    /// - manual: 每帧以 command 匹配; auto: trigger_in 槽位值边沿检测, 未激活帧不写任何槽位
    /// - 分派 (对齐前端 runMatch): string 规则命中 → text 字符串槽位 + matched 数值槽位
    ///   (value 不覆盖); number 命中/miss → value + matched 数值槽位 (text 不覆盖)
    Trigger {
        node_id: String,
        mode: String,
        edge: String,
        default_miss: f32,
        default_miss_text: String,
        command: String,
        rules: Vec<TriggerRuleDef>,
        /// auto 模式 "trigger" 输入端口的上游槽位 (None = 未连接, 与缺省 0.0 对应)
        trigger_in: Option<usize>,
        value: usize,
        matched: usize,
        text: usize,
    },
    /// TextInput: 文本输入源 — 参数 text 每帧原样写入 out 字符串槽位 (覆盖写)
    TextInput { text: String, out: usize },
}

/// 评估单元 — 图内独立路径 (连通分量) 的执行产物, 见 `units` 模块切分
///
/// 单元 0 恒为 prelude (供给节点 op: ProtocolSource/Input/Custom/TextInput),
/// 其余为计算节点连通分量。单元写集互斥; 跨单元读只发生在
/// "计算单元读 prelude 槽位", 由执行方保证 prelude 先行 (并发时每槽位副本先跑 prelude)。
pub struct EvalUnit {
    /// 本单元在扁平 `SlotPlan::ops` 中的区段 `[op_start, op_start + op_len)`
    pub op_start: u32,
    pub op_len: u32,
    /// 每帧评估前需清零的 f32 槽位 (= 本单元全部写槽位, 均为正本)
    pub clear_slots: Vec<u32>,
    /// 每帧评估前需清零的字符串槽位 (同上)
    pub clear_str_slots: Vec<u32>,
    /// 本单元引用的 Filter 节点 id (运行时按单元切分状态 map)
    pub filter_ids: Vec<Box<str>>,
    /// 本单元引用的 Ifft 节点 id
    pub ifft_ids: Vec<Box<str>>,
    /// 本单元引用的 Trigger 节点 id
    pub trigger_ids: Vec<Box<str>>,
    /// 桶分配权重估计 (LPT 贪心装箱)
    pub weight: u32,
}

/// op 桶分配权重估计 — 相对开销启发式 (Math/供给 ≈1, 状态/块运算更重)
pub const fn op_weight(op: &CompiledOp) -> u32 {
    match op {
        CompiledOp::Filter { .. } => 4,
        CompiledOp::FrameDecoder { .. } => 8,
        CompiledOp::Ifft { .. } => 16,
        CompiledOp::Str { .. } | CompiledOp::Trigger { .. } => 2,
        CompiledOp::Math { .. }
        | CompiledOp::ProtocolSource { .. }
        | CompiledOp::ProtocolSourceStr { .. }
        | CompiledOp::Input { .. }
        | CompiledOp::Custom { .. }
        | CompiledOp::TextInput { .. }
        | CompiledOp::TextOut { .. } => 1,
    }
}

/// TextOut 发送规格 — 编译期收集 (TextOut 发送 ticker / 手动命令共用)
#[derive(Debug, Clone)]
pub struct TextOutSpec {
    /// TextOut 节点 id (graph_string_outputs 的键)
    pub node_id: Box<str>,
    /// 目标 Transport 全局节点 id
    pub target_transport: Box<str>,
    /// 换行后缀 (编译期从 [`NewlineMode`] 解出)
    pub newline_suffix: &'static str,
    /// 自动发送最小间隔 ms
    pub min_interval_ms: u32,
}

impl TextOutSpec {
    /// 从 TextOut 节点定义构造规格
    pub fn from_kind(
        node_id: &str,
        target_transport: &str,
        newline: NewlineMode,
        min_interval_ms: u32,
    ) -> Self {
        Self {
            node_id: node_id.into(),
            target_transport: target_transport.into(),
            newline_suffix: newline.suffix(),
            min_interval_ms,
        }
    }
}
