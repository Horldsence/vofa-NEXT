//! 慢路径图求值 + 节点查询 — CompiledGraph::evaluate / evaluate_into / 配置访问器
//!
//! 逐节点 map 语义求值 (相对 [`CompiledEval::run`] 的槽位快路径):
//! - 语义参考实现, 用于单帧调试评估与槽位路径的等价性校验
//! - 帧来源: `source_frames` 多源最新帧缓存 (key = 全局 Protocol 节点 id),
//!   ProtocolSource 节点从对应源的 channels 读值; 源缺失/越界写 0.0
//!
//! [`CompiledEval::run`]: crate::eval::CompiledEval::run

use std::collections::HashMap;

use dsp_fft::{IfftState, SpectrumOutput};
use dsp_filter::DigitalFilter;
use dsp_window::WindowType;

use node_frame_decoder::FrameParser;
use node_kind::{DecoderBlockDef, NodeKind, PortDomain, StrResult};
use node_trigger::TriggerState;

use crate::compile::CompiledGraph;
use crate::eval::{
    node_out_entry, node_out_str_entry, set_port, set_str_port, str_num_default, SourceFramesMap,
};
use crate::{StringValuesMap, ValuesMap};

impl CompiledGraph {
    /// 评估图 — 给定多源最新帧 + 输入值 + Custom 回传值 + Filter 状态 + Decoder 状态,
    /// 返回所有节点的输出端口值
    ///
    /// 返回: HashMap<widgetId, HashMap<portId, f32>>
    ///   - 包含 ProtocolSource/Input/Math/Custom/Filter/FrameDecoder/Ifft/Str/Trigger 的输出
    ///     (Str 与 Trigger 的 F32 域结果写入返回值, String 域结果写入 `out_str`)
    ///   - 不包含 Sink / SpectrumSink / Transport / Protocol (无值平面输出)
    ///
    /// `source_frames`: 多源 latest-value 融合缓存 — 每个 Protocol 源独立缓存
    ///   最近一帧, ProtocolSource 求值时按 node_id 从对应源读取通道值。
    ///
    /// `filter_states`: 滤波器状态 (跨帧持久化), key = Filter 节点 id
    ///   首次遇到 Filter 节点时按其 kind 创建 DigitalFilter 并存入;
    ///   后续帧复用同一状态, 实现逐点滤波的连续性。
    ///   当 Filter 节点的 kind 变化时 (用户修改配置), 自动重建状态。
    ///
    /// `decoder_states`: 帧解码器状态 (跨帧持久化), key = FrameDecoder 节点 id
    ///   由调用方 (data_loop) 喂入字节流并更新 last_frame。
    ///   evaluate 阶段仅读取 last_frame 缓存的 outputs + 附加端口 (valid/frame_count/last_timestamp/fps)。
    ///
    /// `trigger_states`: Trigger 节点状态 (跨帧持久化), key = Trigger 节点 id
    ///   首次遇到 Trigger 节点时按其配置创建 TriggerState 并存入;
    ///   匹配器相关配置 (rules/default_miss/default_miss_text) 变化时自动重建
    ///   (同 Filter 的 kind 变化重建); mode/edge/command 求值时现读, 不触发重建。
    pub fn evaluate(
        &self,
        source_frames: &SourceFramesMap,
        input_values: &HashMap<String, f32>,
        custom_outputs: &HashMap<String, HashMap<String, f32>>,
        filter_states: &mut HashMap<String, DigitalFilter>,
        decoder_states: &HashMap<String, FrameParser>,
        ifft_states: &mut HashMap<String, IfftState>,
        trigger_states: &mut HashMap<String, TriggerState>,
        out_str: &mut StringValuesMap,
    ) -> ValuesMap {
        let mut out = ValuesMap::default();
        self.evaluate_into(
            source_frames,
            input_values,
            custom_outputs,
            filter_states,
            decoder_states,
            ifft_states,
            trigger_states,
            &mut out,
            out_str,
        );
        out
    }

    /// 评估图 (零分配快路径) — 结果写入调用方提供的 `out`
    ///
    /// 与 evaluate 语义相同, 但稳态下 (图结构不变) 几乎无堆分配:
    /// - `out` 内外层 HashMap 跨帧复用 (调用方每帧传入同一 map, 本函数按节点覆盖写)
    /// - 输入端口名用编译期缓存 (in_names) 或 &'static str
    /// - input_index 嵌套查询零分配
    ///
    /// 注意: 本函数只覆盖写当前节点的端口, 不清理过期键 — 图结构变化 (重编译)
    /// 时调用方应清空 out (process_frames_batch 通过 graphs_version 检测)。
    /// `out_str` 同理: Str 节点 String 域结果写入 (未连接字符串输入按 "" 处理)。
    #[allow(clippy::too_many_arguments, clippy::cast_precision_loss)]
    pub fn evaluate_into(
        &self,
        source_frames: &SourceFramesMap,
        input_values: &HashMap<String, f32>,
        custom_outputs: &HashMap<String, HashMap<String, f32>>,
        filter_states: &mut HashMap<String, DigitalFilter>,
        decoder_states: &HashMap<String, FrameParser>,
        ifft_states: &mut HashMap<String, IfftState>,
        trigger_states: &mut HashMap<String, TriggerState>,
        out: &mut ValuesMap,
        out_str: &mut StringValuesMap,
    ) {
        for node_id in &self.eval_order {
            let Some(node) = self.nodes.get(node_id) else {
                continue;
            };

            match &node.kind {
                NodeKind::ProtocolSource {
                    node_id: source_id,
                    channels,
                    port_names,
                } => {
                    let frame = source_frames.get(source_id);
                    let m = node_out_entry(out, node_id);
                    let names =
                        node_kind::protocol_source_port_names(port_names.as_deref(), *channels);
                    for (i, name) in names.iter().enumerate() {
                        let v = frame
                            .and_then(|f| f.channels.get(i))
                            .copied()
                            .unwrap_or(0.0);
                        set_port(m, name, v);
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
                    let need_rebuild = filter_states.get(node_id).is_none_or(|f| f.kind() != kind);
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
                NodeKind::Ifft => {
                    // 环形播放重建后的时域采样 (buffer 由 spectrum_ticker 合成)
                    let v = ifft_states
                        .get_mut(node_id)
                        .map_or(0.0, dsp_fft::IfftState::next_sample);
                    let m = node_out_entry(out, node_id);
                    set_port(m, "out0", v);
                }
                NodeKind::Str { op, num } => {
                    // 输入按 StrOp::input_ports() 端口表顺序紧凑拆分收集
                    // (与 CompiledOp::Str 的对齐约定一致):
                    // - String 端口 → str_inputs: 经 string_input_index 查上游, 无边/上游无值 = ""
                    // - F32 端口 → num_inputs: 有边走上游值, 无边用 num 内联回退 (按端口名映射)
                    // 端口表最大 arity: str ≤ 2 / num ≤ 2, 栈数组覆盖, 超出走堆 (防御)
                    let ports = op.input_ports();
                    let n_str = ports.iter().filter(|p| p.1 == PortDomain::String).count();
                    let n_num = ports.len() - n_str;
                    let mut stack_str: [&str; 2] = ["", ""];
                    let mut heap_str;
                    let str_inputs: &mut [&str] = if n_str <= 2 {
                        &mut stack_str[..n_str]
                    } else {
                        heap_str = vec![""; n_str];
                        &mut heap_str
                    };
                    let mut stack_num = [0.0f32; 2];
                    let mut heap_num;
                    let num_inputs: &mut [f32] = if n_num <= 2 {
                        &mut stack_num[..n_num]
                    } else {
                        heap_num = vec![0.0; n_num];
                        &mut heap_num
                    };
                    let (mut si, mut ni) = (0, 0);
                    for (name, domain) in ports {
                        match domain {
                            PortDomain::String => {
                                str_inputs[si] = self.resolve_str_input(node_id, name, out_str);
                                si += 1;
                            }
                            PortDomain::F32 => {
                                num_inputs[ni] = if self
                                    .input_index
                                    .get(node_id)
                                    .is_some_and(|p| p.contains_key(*name))
                                {
                                    self.resolve_input(node_id, name, out)
                                } else {
                                    str_num_default(num, name)
                                };
                                ni += 1;
                            }
                            PortDomain::Bytes => {} // Str 端口表无 Bytes, 防御
                        }
                    }
                    match op.evaluate(str_inputs, num_inputs) {
                        StrResult::Text(t) => {
                            let m = node_out_str_entry(out_str, node_id);
                            set_str_port(m, "result", &t);
                        }
                        StrResult::Num(v) => {
                            let m = node_out_entry(out, node_id);
                            set_port(m, "result", v);
                        }
                    }
                }
                NodeKind::Trigger {
                    mode,
                    edge,
                    default_miss,
                    default_miss_text,
                    command,
                    rules,
                } => {
                    // 懒初始化 / 配置变更重建 (仿 Filter arm; 仅匹配器相关字段参与比较)
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
                    // manual: 每帧以 command 匹配; auto: "trigger" 输入上游值边沿检测,
                    // 未激活帧不产出 (对齐前端 useEffect: 不调用 runMatch → 端口保持上次值)
                    let result = if mode == "auto" {
                        let tv = self.resolve_input(node_id, "trigger", out);
                        state.eval_auto(edge, tv)
                    } else {
                        Some(state.eval_manual(command))
                    };
                    // 分派对齐前端 runMatch: string 规则命中 → text 写字符串平面
                    // (value 不覆盖); number 命中/miss → value 写数值平面
                    // (text 不覆盖, 保持上次字符串值); matched 两种情形都写
                    if let Some(r) = result {
                        let m = node_out_entry(out, node_id);
                        if r.output_type == "string" {
                            let sm = node_out_str_entry(out_str, node_id);
                            set_str_port(sm, "text", &r.text);
                        } else {
                            set_port(m, "value", r.value);
                        }
                        set_port(m, "matched", if r.matched { 1.0 } else { 0.0 });
                    }
                }
                NodeKind::Sink
                | NodeKind::SpectrumSink { .. }
                | NodeKind::Transport { .. }
                | NodeKind::Protocol { .. } => {
                    // 无值平面输出的节点不应出现在 eval_order 中, 防御性跳过
                }
            }
        }
    }

    /// 解析某节点某输入端口的上游输出值
    /// (在 evaluate 过程中, 上游必然已计算完成)
    fn resolve_input(&self, node_id: &str, port_id: &str, computed: &ValuesMap) -> f32 {
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

    /// 解析某节点某字符串输入端口的上游输出值
    /// (经 string_input_index; 无边 / 上游本帧未产出 (如 auto Trigger 未激活) → "")
    fn resolve_str_input<'a>(
        &self,
        node_id: &str,
        port_id: &str,
        computed_str: &'a StringValuesMap,
    ) -> &'a str {
        if let Some((src_node, src_port)) = self
            .string_input_index
            .get(node_id)
            .and_then(|ports| ports.get(port_id))
        {
            computed_str
                .get(src_node)
                .and_then(|m| m.get(src_port))
                .map_or("", String::as_str)
        } else {
            ""
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
    pub fn collect_spectrum_inputs(&self, computed: &ValuesMap) -> HashMap<String, f32> {
        let mut result = HashMap::new();
        for (node_id, node) in &self.nodes {
            if matches!(node.kind, NodeKind::SpectrumSink { .. }) {
                let val = self.resolve_input(node_id, "in0", computed);
                result.insert(node_id.clone(), val);
            }
        }
        result
    }
}

// ============ 节点查询访问器 ============

impl CompiledGraph {
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

    /// 获取所有 Ifft 节点 id (供状态清理 + spectrum_ticker 合成时域缓冲)
    pub fn ifft_node_ids(&self) -> Vec<String> {
        self.nodes
            .iter()
            .filter(|(_, n)| matches!(n.kind, NodeKind::Ifft))
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// 解析 Ifft 节点的上游 FFT (SpectrumSink) 节点 id
    ///
    /// 输入端口固定为 "spectrum" (频域), 编译期从 input_index 反查边:
    /// (source 节点的 "spectrum" 输出) → source 节点 id。
    /// 无上游边返回 None。
    pub fn ifft_source(&self, node_id: &str) -> Option<String> {
        self.input_index
            .get(node_id)
            .and_then(|ports| ports.get("spectrum"))
            .map(|(src, _)| src.clone())
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
    /// 用于 decoder_feed 在节点变更时重建 FrameParser
    ///
    /// 注意: 返回的 loopback 标志为 deprecated (见 NodeKind::FrameDecoder),
    /// 新语义下字节来源完全由输入字节边决定 (见 byte_plan)。
    #[allow(clippy::type_complexity)]
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
}
