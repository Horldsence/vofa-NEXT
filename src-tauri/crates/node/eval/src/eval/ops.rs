//! 逐帧评估执行体 — run / run_unit_frame / run_ops (op 解释器)

use std::collections::HashMap;

use dsp_fft::IfftState;
use dsp_filter::DigitalFilter;
use frame_decoder::FrameParser;
use kind::StrResult;
use lower::CompiledOp;
use trigger::TriggerState;
use vofa_core::DataFrame;

use super::{CompiledEval, SourceFramesMap, SourceTextsMap};

impl CompiledEval {
    /// 逐帧评估全部 ops (现状入口, 调用方负责整表清零) — 内部走 [`Self::run_ops`]
    ///
    /// `source_frames`: 多源最新帧缓存 (key = Protocol 节点 id),
    ///   语义为 latest-value 融合 — 每个源独立缓存最近一帧, 本函数逐源读取;
    ///   源缺失或通道越界时对应端口不写；真实 0.0 仍是有效样本。
    /// `source_texts`: 每源最新文本缓存 (key = Protocol 节点 id, RawData 协议写入),
    ///   ProtocolSource 的 "str" 端口 (String 域) 从此读取; 源无缓存时对应槽位不写
    ///   (快照保持上次值, 对齐 Trigger 未激活帧语义)。
    /// `slots` / `written` 由调用方分配 (长度 == slot_count) 并跨帧复用;
    /// `str_slots` / `str_written` 同理 (长度 == str_slot_count, 字符串缓冲跨帧复用分配)。
    /// `trigger_states`: Trigger 节点状态 (跨帧持久化, key = Trigger 节点 id) —
    ///   懒建 / 配置变更重建, 语义与 filter_states 一致 (见 evaluate_into)。
    /// 调用方负责每帧清零 (slots/str_slots 防上帧值泄漏, written/str_written 复刻
    /// "本帧未产出 = 键不存在")。
    /// op 写槽位时置位 written — FrameDecoder 无 parser / Custom 无回传以外的
    /// 缺失都不写 (与 evaluate_into 的 map 语义一致)。
    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &self,
        source_frames: &SourceFramesMap,
        source_texts: &SourceTextsMap,
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
        let resolved = self.resolve_frames(source_frames, None);
        self.run_ops(
            &self.plan.ops,
            resolved.as_slice(),
            source_texts,
            input_values,
            custom_outputs,
            filter_states,
            decoder_states,
            ifft_states,
            trigger_states,
            slots,
            written,
            str_slots,
            str_written,
        );
    }

    /// 单帧单单元评估 (并发路径): 清本单元写槽位 → 执行单元 op 区段
    ///
    /// 单元写集互斥 (编译期切分不变量); 单元内读要么来自本单元先写槽位
    /// (单元内拓扑序), 要么来自 prelude 槽位 (调用方保证本帧 prelude 先行)。
    /// 调用方保证 `slots/written` 等缓冲按图独占 — 并发时每桶一份槽位副本。
    #[allow(clippy::too_many_arguments)]
    pub fn run_unit_frame(
        &self,
        unit: &lower::EvalUnit,
        resolved: &[Option<&DataFrame>],
        source_texts: &SourceTextsMap,
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
        for &s in &unit.clear_slots {
            let i = s as usize;
            slots[i] = 0.0;
            written[i] = false;
        }
        for &s in &unit.clear_str_slots {
            let i = s as usize;
            str_slots[i].clear();
            str_written[i] = false;
        }
        self.run_ops(
            &self.plan.ops[unit.op_start as usize..][..unit.op_len as usize],
            resolved,
            source_texts,
            input_values,
            custom_outputs,
            filter_states,
            decoder_states,
            ifft_states,
            trigger_states,
            slots,
            written,
            str_slots,
            str_written,
        );
    }

    /// op 序列执行体 — 帧源已由调用方预解析 (`resolved` 与 plan.frame_sources 对齐)
    ///
    /// `ops` 可为全体 ops (串行路径) 或单元区段 (并发路径); 逐帧评估纯数组读写,
    /// 唯一字符串查找是 ProtocolSourceStr 的文本缓存读取。
    #[allow(clippy::too_many_arguments, clippy::cast_precision_loss)]
    pub fn run_ops(
        &self,
        ops: &[CompiledOp],
        resolved: &[Option<&DataFrame>],
        source_texts: &SourceTextsMap,
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
        for op in ops {
            match op {
                CompiledOp::ProtocolSource { src, ch, slot } => {
                    if let Some(value) = resolved[*src].and_then(|f| f.channels.get(*ch)) {
                        slots[*slot] = *value;
                        written[*slot] = true;
                    }
                }
                CompiledOp::ProtocolSourceStr { src, slot } => {
                    // 源有缓存文本时写字符串槽位 (复用缓冲原位写, 仿 TextInput);
                    // 无缓存时不写 (str_written 不置位 → 快照保持上次值)
                    if let Some(text) = source_texts.get(&self.plan.frame_sources[*src]) {
                        let s = &mut str_slots[*slot];
                        s.clear();
                        s.push_str(text);
                        str_written[*slot] = true;
                    }
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
                    config,
                    input,
                    out,
                } => {
                    let input_val = input.map_or(0.0, |s| slots[s]);
                    // 派生 owned FilterKind (避免与 filter_states 借用重叠),
                    // config 变化重建滤波器状态 (与原 kind 变更语义一致)。
                    let new_kind = config;
                    let need_rebuild = filter_states
                        .get(node_id)
                        .is_none_or(|f| f.kind() != new_kind);
                    if need_rebuild {
                        filter_states.insert(node_id.clone(), DigitalFilter::new(new_kind.clone()));
                    }
                    let filter = filter_states
                        .get_mut(node_id)
                        .expect("filter 状态已在上方缺失时插入");
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
                    str_defaults,
                    num_inputs,
                    num_defaults,
                    text_out,
                    num_out,
                } => {
                    // 输入收集: None 字符串槽位 = str_defaults[i] (未连接 → 内联回退文本,
                    // 当前仅 FORMAT 的 fmt 端口取模板, 其余空串 — 与 evaluate_into 一致),
                    // None 数值槽位 = num_defaults[i] (内联回退) — 与 evaluate_into 一致。
                    // 端口表最大 arity: str ≤ 2 / num ≤ 2 (FORMAT 数值 4 路走堆兜底)
                    let mut stack_str: [&str; 2] = ["", ""];
                    let mut heap_str;
                    let str_buf: &mut [&str] = if str_inputs.len() <= 4 {
                        &mut stack_str[..str_inputs.len()]
                    } else {
                        heap_str = vec![""; str_inputs.len()];
                        &mut heap_str
                    };
                    for (i, s) in str_inputs.iter().enumerate() {
                        str_buf[i] = s
                            .map_or(str_defaults.get(i).map_or("", String::as_str), |slot| {
                                str_slots[slot].as_str()
                            });
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
                    let state = trigger_states
                        .get_mut(node_id)
                        .expect("trigger 状态已在上方缺失时插入");
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
                CompiledOp::TextOut { input, out } => {
                    // 上游字符串透传写本节点槽位 (未连接不写 → 无值不发)
                    if let Some(src) = input {
                        if src != out {
                            // 不同槽位: split_at_mut 同时可变借用两端, 零分配拷贝
                            let (lo, hi) = (*src.min(out), *src.max(out));
                            let (low, high) = str_slots.split_at_mut(hi);
                            let (from, to) = if *src == lo {
                                (&mut low[lo], &mut high[0])
                            } else {
                                (&mut high[0], &mut low[lo])
                            };
                            to.clear();
                            to.push_str(from);
                        }
                        str_written[*out] = true;
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
}
