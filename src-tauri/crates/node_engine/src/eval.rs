//! 编译期槽位评估表 (CompiledEval) — f32 热路径
//!
//! 结构: 平坦 [`CompiledOp`] 数组 + 槽位读写 + 零字符串哈希。
//! 逐帧评估时仅有的字符串查找是 ProtocolSource 的帧源解析
//! (每源每帧一次, 编译期预排为 `frame_sources` 下标表)。

use std::collections::HashMap;

use dsp_fft::IfftState;
use dsp_filter::{DigitalFilter, FilterKind};
use rustc_hash::FxBuildHasher;
use vofa_core::DataFrame;

use node_frame_decoder::FrameParser;
use node_kind::{MathOp, StrNumParams, StrOp, StrResult};
use node_trigger::{TriggerRuleDef, TriggerState};

use crate::{StringValuesMap, ValuesMap};

/// 多源最新帧缓存 — key = 全局 Protocol 节点 id, value = 该源最近一帧
/// (latest-value 融合: 每个源独立缓存, 求值时按源读取)
pub type SourceFramesMap = HashMap<String, DataFrame, FxBuildHasher>;

/// 取节点的输出 map (不存在则创建) — evaluate_into 热路径用
///
/// 不做 clear: 端口覆盖写, 稳态零分配; 过期端口清理由调用方
/// 在图重编译时清空整个 out 保证。
pub fn node_out_entry<'a>(
    out: &'a mut ValuesMap,
    node_id: &str,
) -> &'a mut HashMap<String, f32, FxBuildHasher> {
    if out.get_mut(node_id).is_none() {
        out.insert(node_id.to_string(), HashMap::default());
    }
    out.get_mut(node_id).unwrap()
}

/// 写端口值 — 键已存在时原位写 (零分配), 不存在才插入
pub fn set_port(m: &mut HashMap<String, f32, FxBuildHasher>, port: &str, value: f32) {
    if let Some(slot) = m.get_mut(port) {
        *slot = value;
    } else {
        m.insert(port.to_string(), value);
    }
}

/// 取节点的字符串输出 map (不存在则创建) — 仿 [`node_out_entry`]
pub fn node_out_str_entry<'a>(
    out: &'a mut StringValuesMap,
    node_id: &str,
) -> &'a mut HashMap<String, String, FxBuildHasher> {
    if out.get_mut(node_id).is_none() {
        out.insert(node_id.to_string(), HashMap::default());
    }
    out.get_mut(node_id).unwrap()
}

/// 写字符串端口值 — 键已存在时原位写 (复用缓冲, 稳态低分配), 不存在才插入
pub fn set_str_port(m: &mut HashMap<String, String, FxBuildHasher>, port: &str, value: &str) {
    if let Some(slot) = m.get_mut(port) {
        slot.clear();
        slot.push_str(value);
    } else {
        m.insert(port.to_string(), value.to_owned());
    }
}

/// Str 数值端口的内联回退值 (端口未连接时使用): 端口名 → [`StrNumParams`] 字段
pub fn str_num_default(num: &StrNumParams, port: &str) -> f32 {
    match port {
        "pos" => num.pos,
        "len" => num.len,
        "size" => num.size,
        _ => 0.0,
    }
}

/// 编译期槽位操作 — 平坦操作序列 (拓扑序 == eval_order), 逐帧评估零字符串哈希
pub enum CompiledOp {
    /// ProtocolSource: source_frames[frame_sources[src]].channels[ch] → slot
    /// (源缺失/通道越界写 0.0, 与未连接语义一致)
    ProtocolSource { src: usize, ch: usize, slot: usize },
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
    /// Ifft: 读 ifft_states[node_id] 的下一个重建采样 → out 槽位 (环形播放, 时域)
    Ifft { node_id: String, out: usize },
    /// Str: 按 StrOp::input_ports() 端口表紧凑拆分输入 (只含同 domain 端口, 按端口表顺序):
    /// - str_inputs[i] = 第 i 个 String 端口的上游字符串槽位 (None = 未连接/上游无槽位 → "")
    /// - num_inputs[i] = 第 i 个 F32 端口的上游数值槽位 (None → num_defaults[i])
    /// - num_defaults[i] = 第 i 个 F32 端口的内联回退值 (编译期从 StrNumParams 捕获)
    ///
    /// 输出按 StrOp::output_domain(): String → text_out 字符串槽位, F32 → num_out 数值槽位
    Str {
        op: StrOp,
        str_inputs: Vec<Option<usize>>,
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

/// 编译期槽位评估表 — CompiledGraph::compile 时构建, 逐帧评估纯数组读写
pub struct CompiledEval {
    /// 槽位 i 对应的 (node_id, port) — 供快照物化/派生边反查
    pub(crate) slot_names: Vec<(String, String)>,
    /// (node_id, port) → 槽位下标
    pub(crate) slot_index: HashMap<(String, String), usize, FxBuildHasher>,
    /// 平坦操作序列 (拓扑序 == eval_order)
    pub(crate) ops: Vec<CompiledOp>,
    /// SpectrumSink 输入槽位: (sink_node_id, 源值槽位; None = 无上游边, 与缺省 0.0 对应)
    pub(crate) spectrum_slots: Vec<(String, Option<usize>)>,
    /// ProtocolSource 引用的全局 Protocol 节点 id 表 (去重, 编译期预排;
    /// 逐帧评估时每源一次字符串查找解析为帧引用, op 用下标直读)
    pub(crate) frame_sources: Vec<String>,
    /// 字符串槽位 i 对应的 (node_id, port) — Str 节点 String 域输出, 仿 slot_names
    pub(crate) str_slot_names: Vec<(String, String)>,
    /// (node_id, port) → 字符串槽位下标
    pub(crate) str_slot_index: HashMap<(String, String), usize, FxBuildHasher>,
}

impl CompiledEval {
    /// 槽位数 (调用方据此分配 slots/written 缓冲并跨帧复用)
    pub const fn slot_count(&self) -> usize {
        self.slot_names.len()
    }

    /// 字符串槽位数 (调用方据此分配 str_slots/str_written 缓冲并跨帧复用)
    pub const fn str_slot_count(&self) -> usize {
        self.str_slot_names.len()
    }

    /// (node_id, port) → 槽位 (派生边批首解析用)
    pub fn slot_of(&self, node: &str, port: &str) -> Option<usize> {
        self.slot_index
            .get(&(node.to_string(), port.to_string()))
            .copied()
    }

    /// (node_id, port) → 字符串槽位 (字符串输出发布解析用)
    pub fn str_slot_of(&self, node: &str, port: &str) -> Option<usize> {
        self.str_slot_index
            .get(&(node.to_string(), port.to_string()))
            .copied()
    }

    /// 逐帧评估: 纯数组读写, 零字符串哈希
    /// (唯一例外: 帧源解析 — 每个被引用 Protocol 源每帧一次 HashMap 查找)
    ///
    /// `source_frames`: 多源最新帧缓存 (key = Protocol 节点 id),
    ///   语义为 latest-value 融合 — 每个源独立缓存最近一帧, 本函数逐源读取;
    ///   源缺失或通道越界时对应端口写 0.0 (与未连接语义一致)。
    /// `slots` / `written` 由调用方分配 (长度 == slot_count) 并跨帧复用;
    /// `str_slots` / `str_written` 同理 (长度 == str_slot_count, 字符串缓冲跨帧复用分配)。
    /// `trigger_states`: Trigger 节点状态 (跨帧持久化, key = Trigger 节点 id) —
    ///   懒建 / 配置变更重建, 语义与 filter_states 一致 (见 evaluate_into)。
    /// 调用方负责每帧清零 (slots/str_slots 防上帧值泄漏, written/str_written 复刻
    /// "本帧未产出 = 键不存在")。
    /// op 写槽位时置位 written — FrameDecoder 无 parser / Custom 无回传以外的
    /// 缺失都不写 (与 evaluate_into 的 map 语义一致)。
    #[allow(clippy::too_many_arguments, clippy::cast_precision_loss)]
    pub fn run(
        &self,
        source_frames: &SourceFramesMap,
        input_values: &HashMap<String, f32>,
        custom_outputs: &HashMap<String, HashMap<String, f32>>,
        filter_states: &mut HashMap<String, DigitalFilter>,
        decoder_states: &HashMap<String, FrameParser>,
        ifft_states: &mut HashMap<String, IfftState>,
        trigger_states: &mut HashMap<String, TriggerState>,
        slots: &mut [f32],
        written: &mut [bool],
        str_slots: &mut [String],
        str_written: &mut [bool],
    ) {
        // 帧源预解析: 每源每帧一次字符串哈希, 之后 op 用下标零开销直读
        // (8 源以内走栈数组, 避免逐帧堆分配)
        let mut stack_src: [Option<&DataFrame>; 8] = [None; 8];
        let mut heap_src;
        let resolved: &mut [Option<&DataFrame>] = if self.frame_sources.len() <= 8 {
            &mut stack_src[..self.frame_sources.len()]
        } else {
            heap_src = vec![None; self.frame_sources.len()];
            &mut heap_src
        };
        for (i, id) in self.frame_sources.iter().enumerate() {
            resolved[i] = source_frames.get(id);
        }

        for op in &self.ops {
            match op {
                CompiledOp::ProtocolSource { src, ch, slot } => {
                    slots[*slot] = resolved[*src]
                        .and_then(|f| f.channels.get(*ch))
                        .copied()
                        .unwrap_or(0.0);
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
                        buf[i] = s.map_or(0.0, |s| slots[s]);
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
                    let input_val = input.map_or(0.0, |s| slots[s]);
                    // 懒初始化 / kind 变化时重建滤波器状态 (与 evaluate_into 一致)
                    let need_rebuild = filter_states.get(node_id).is_none_or(|f| f.kind() != kind);
                    if need_rebuild {
                        filter_states.insert(node_id.clone(), DigitalFilter::new(kind.clone()));
                    }
                    let filter = filter_states.get_mut(node_id).unwrap();
                    slots[*out] = filter.process(input_val);
                    written[*out] = true;
                }
                CompiledOp::Ifft { node_id, out } => {
                    // 环形播放重建后的时域采样 (buffer 由 spectrum_ticker 合成)
                    slots[*out] = ifft_states
                        .get_mut(node_id)
                        .map_or(0.0, dsp_fft::IfftState::next_sample);
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
                CompiledOp::Str {
                    op,
                    str_inputs,
                    num_inputs,
                    num_defaults,
                    text_out,
                    num_out,
                } => {
                    // 输入收集: None 字符串槽位 = "" (未连接/上游无值),
                    // None 数值槽位 = num_defaults[i] (内联回退) — 与 evaluate_into 一致。
                    // 端口表最大 arity: str ≤ 2 / num ≤ 2, 栈数组覆盖, 超出走堆 (防御)
                    let mut stack_str: [&str; 2] = ["", ""];
                    let mut heap_str;
                    let str_buf: &mut [&str] = if str_inputs.len() <= 2 {
                        &mut stack_str[..str_inputs.len()]
                    } else {
                        heap_str = vec![""; str_inputs.len()];
                        &mut heap_str
                    };
                    for (i, s) in str_inputs.iter().enumerate() {
                        str_buf[i] = s.map_or("", |s| str_slots[s].as_str());
                    }
                    let mut stack_num = [0.0f32; 2];
                    let mut heap_num;
                    let num_buf: &mut [f32] = if num_inputs.len() <= 2 {
                        &mut stack_num[..num_inputs.len()]
                    } else {
                        heap_num = vec![0.0; num_inputs.len()];
                        &mut heap_num
                    };
                    for (i, s) in num_inputs.iter().enumerate() {
                        num_buf[i] = s.map_or(num_defaults[i], |s| slots[s]);
                    }
                    match op.evaluate(str_buf, num_buf) {
                        StrResult::Text(t) => {
                            if let Some(o) = text_out {
                                str_slots[*o] = t;
                                str_written[*o] = true;
                            }
                        }
                        StrResult::Num(v) => {
                            if let Some(o) = num_out {
                                slots[*o] = v;
                                written[*o] = true;
                            }
                        }
                    }
                }
                CompiledOp::Trigger {
                    node_id,
                    mode,
                    edge,
                    default_miss,
                    default_miss_text,
                    command,
                    rules,
                    trigger_in,
                    value,
                    matched,
                    text,
                } => {
                    // 懒初始化 / 配置变更重建 (与 evaluate_into 的 Trigger arm 一致)
                    let need_rebuild = trigger_states
                        .get(node_id)
                        .is_none_or(|s| !s.matches_config(rules, *default_miss, default_miss_text));
                    if need_rebuild {
                        trigger_states.insert(
                            node_id.clone(),
                            TriggerState::new(
                                rules.clone(),
                                *default_miss,
                                default_miss_text.clone(),
                            ),
                        );
                    }
                    let state = trigger_states.get_mut(node_id).unwrap();
                    // manual: 每帧以 command 匹配; auto: 边沿检测, 未激活帧不产出
                    // 两种模式都先取 "trigger" 输入槽位值 (与 evaluate_into 一致):
                    // auto 用于边沿检测; manual 也要同步 prev (前端 useEffect 在非
                    // auto 模式仍每帧跟踪 prevTriggerRef)
                    let tv = trigger_in.map_or(0.0, |s| slots[s]);
                    let result = if mode == "auto" {
                        state.eval_auto(edge, tv)
                    } else {
                        state.record_prev(tv);
                        Some(state.eval_manual(command))
                    };
                    // 分派对齐前端 runMatch: string 命中 → text 字符串槽位 (value 不覆盖);
                    // number 命中/miss → value 数值槽位 (text 不覆盖); matched 两种情形都写
                    if let Some(r) = result {
                        if r.output_type == "string" {
                            str_slots[*text] = r.text;
                            str_written[*text] = true;
                        } else {
                            slots[*value] = r.value;
                            written[*value] = true;
                        }
                        slots[*matched] = if r.matched { 1.0 } else { 0.0 };
                        written[*matched] = true;
                    }
                }
                CompiledOp::TextInput { text, out } => {
                    // 参数 text 原样写入字符串槽位 (复用缓冲原位写, 仿 set_str_port)
                    let slot = &mut str_slots[*out];
                    slot.clear();
                    slot.push_str(text);
                    str_written[*out] = true;
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

    /// 字符串快照物化: str_slots + str_written → StringValuesMap (仅快照发布点调用)
    ///
    /// 只物化 written 置位的槽位, 不清理过期键 (与 materialize / evaluate_into 语义一致)
    pub fn materialize_str(
        &self,
        str_slots: &[String],
        str_written: &[bool],
        out_str: &mut StringValuesMap,
    ) {
        for (i, (node_id, port)) in self.str_slot_names.iter().enumerate() {
            if str_written[i] {
                let m = node_out_str_entry(out_str, node_id);
                set_str_port(m, port, &str_slots[i]);
            }
        }
    }

    /// SpectrumSink 输入: (sink_id, value) 迭代, 仅 written 槽位
    pub fn spectrum_values<'a>(
        &'a self,
        slots: &'a [f32],
        written: &'a [bool],
    ) -> impl Iterator<Item = (&'a str, f32)> + 'a {
        self.spectrum_slots
            .iter()
            .filter_map(move |(sink, slot)| match slot {
                Some(s) if written[*s] => Some((sink.as_str(), slots[*s])),
                _ => None,
            })
    }
}

// 测试模块已迁移至 src/equiv_tests.rs / src/eval_tests.rs (顶层 #[cfg(test)])
