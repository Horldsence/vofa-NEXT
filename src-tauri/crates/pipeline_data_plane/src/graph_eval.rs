//! 数值平面评估 — 槽位热路径 (process_source_batch) + 事件驱动快照评估
//!
//! 两平面重构后:
//! - 热路径按源触发: 某 Protocol 源来帧 → 仅评估"引用该源的 tab 图"与
//!   "无 ProtocolSource 的纯本地图" (后者沿用旧单源行为: 任意来帧都评估);
//!   每帧先把该帧写入 source_frames[source] (其他源保持缓存最新帧, latest-value 融合),
//!   再走 CompiledEval::run 槽位评估 — 调用方式/槽位复用/批内锁粒度与旧版一致
//! - 快照评估 (evaluate_snapshot_now): 字节/输入事件后以 source_frames 现状评估,
//!   取代旧 force_eval 空帧机制

use crate::eval_state::GraphEvalState;
use buffer_databuffer::DataBuffer;
use node_engine::{CompiledGraph, SourceFramesMap};
use node_kind::NodeKind;
use std::collections::HashMap;
use vofa_core::DataFrame;

/// eval 段细分耗时 (纳秒累计, 由调用方汇入数据平面指标)
#[derive(Default)]
pub struct EvalBreakdown {
    pub push_frame_ns: u64,
    pub graph_eval_ns: u64,
    pub derived_ns: u64,
    pub spectrum_ns: u64,
}

/// 每 graph 一组槽位缓冲 (slots, written, str_slots, str_written), 批内跨帧复用
type SlotBufs = (Vec<f32>, Vec<bool>, Vec<String>, Vec<bool>);

/// StringValuesMap (FxHash) 深合并进快照 map (std hasher) — 移动语义, 字符串零 clone
///
/// 同 (node, port) 覆盖写; 两侧 hasher 不同 (FxHash vs SipHash) 故逐条目迁移
fn merge_str_map(
    src: node_engine::StringValuesMap,
    dst: &mut HashMap<String, HashMap<String, String>>,
) {
    for (node_id, ports) in src {
        dst.entry(node_id).or_default().extend(ports);
    }
}

/// 物化当前帧各图的 str 槽位 → 合并进 graph_string_outputs (仅快照发布点调用, 稀疏)
///
/// 覆盖写语义同 f32 materialize: 仅 written 置位槽位物化, 未触发图旧值保留
/// (latest-value 融合); 过期键清理由发布点侧的 graphs_version 检查负责
fn publish_str_slots(
    graph_list: &[&CompiledGraph],
    slot_bufs: &[SlotBufs],
    out: &mut HashMap<String, HashMap<String, String>>,
) {
    let mut buf = node_engine::StringValuesMap::default();
    for (gi, g) in graph_list.iter().enumerate() {
        let (_, _, str_slots, str_written) = &slot_bufs[gi];
        g.compiled()
            .materialize_str(str_slots, str_written, &mut buf);
    }
    merge_str_map(buf, out);
}

/// 图是否被指定源触发:
/// - 引用了该 Protocol 源 (ProtocolSource.node_id == source_id) → 触发
/// - 不含任何 ProtocolSource (Input/Math/Custom 等纯本地图) → 任意源来帧都触发
///   (沿用旧单源架构行为: 所有图每帧评估)
/// - 引用了其他源 → 不触发 (该源来帧时才评估)
fn graph_triggered_by(g: &CompiledGraph, source_id: &str) -> bool {
    let mut has_source = false;
    for n in g.value_nodes() {
        if let NodeKind::ProtocolSource { node_id, .. } = &n.kind {
            has_source = true;
            if node_id == source_id {
                return true;
            }
        }
    }
    !has_source
}

/// 事件驱动快照评估 — 以 source_frames 现状评估所有图并发布 output_snapshot
///
/// 步骤:
/// 1. 对每个图调用 evaluate (传入 filter_states + decoder_states + trigger_states,
///    逐点滤波/解码/触发匹配状态跨帧持久化)
/// 2. 合并所有图输出到 output_snapshot
/// 3. 遍历所有图的 SpectrumSink, 从 output_snapshot 取输入值, push 到对应 analyzer
///
/// 调用时机: FrameDecoder 字节喂入后 / set_input_value / submit_custom_output
/// (取代旧 evaluate_all_graphs_with 的空帧语义 — ProtocolSource 从缓存读最新值)
pub fn evaluate_snapshot_now(eval_state: &GraphEvalState, source_frames: &SourceFramesMap) {
    let input_values = eval_state.input_values.lock().clone();
    let custom_outputs = eval_state.custom_outputs.lock().clone();
    let source_texts = eval_state.source_texts.lock();
    let graphs = eval_state.graphs.lock();
    let mut filter_states = eval_state.filter_states.lock();
    let decoder_states = eval_state.decoder_states.lock();
    let mut ifft_states = eval_state.ifft_states.lock();
    let mut trigger_states = eval_state.trigger_states.lock();

    let mut combined: node_engine::ValuesMap = Default::default();
    // 字符串输出: 各图求值结果累积于此, 求值后全量覆盖写进 graph_string_outputs
    let mut combined_str = node_engine::StringValuesMap::default();
    for (_, graph) in graphs.iter() {
        let out = graph.evaluate(
            source_frames,
            &source_texts,
            &input_values,
            &custom_outputs,
            &mut filter_states,
            &decoder_states,
            &mut ifft_states,
            &mut trigger_states,
            &mut combined_str,
        );
        for (k, v) in out {
            combined.insert(k, v);
        }
    }

    // 更新 output_snapshot (供 60 FPS ticker 推送)
    {
        let mut snap = eval_state.output_snapshot.lock();
        snap.tick = snap.tick.wrapping_add(1);
        snap.values = combined.clone();
    }

    // 更新后端字符串输出 (供 text_output_ticker 合并发布) —
    // 全量覆盖写: combined_str 覆盖所有图, 先物化到本地 map 再整体 swap,
    // 过期节点条目随 swap 清理 (同 snap.values 语义)
    let mut str_map: HashMap<String, HashMap<String, String>> = HashMap::new();
    merge_str_map(combined_str, &mut str_map);
    *eval_state.graph_string_outputs.lock() = str_map;

    // 收集 SpectrumSink 输入值, push 到对应 analyzer 的滑动窗口
    // analyzer 的创建/删除由 spectrum_ticker 在每 tick 开头与 graphs 同步
    let mut analyzers = eval_state.spectrum_analyzers.lock();
    if !analyzers.is_empty() {
        for (_, graph) in graphs.iter() {
            let spectrum_inputs = graph.collect_spectrum_inputs(&combined);
            for (sink_id, value) in spectrum_inputs {
                if let Some(analyzer) = analyzers.get_mut(&sink_id) {
                    analyzer.push(value);
                }
            }
        }
    }
}

/// 单源帧批处理 (热路径) — 一个源的一批帧一次性完成
/// source_frames 更新 + push_frame + 图评估 + 派生值收集
///
/// 与旧 process_frames_batch 语义对应 (每帧: push_frame → evaluate → push_derived,
/// 保证时间戳对齐), 差异:
/// - 仅评估被该源触发的图 (graph_triggered_by), 派生回写进该源自己的 buffer
/// - 每帧先把帧写入 source_frames[source_id] (clone_from 复用分配, 稳态零分配),
///   ProtocolSource 槽位经 CompiledEval::run 从 source_frames 直读
/// - input_values / custom_outputs / source_texts / graphs / filter_states 等锁
///   每批只拿一次 (同旧版)
/// - 槽位缓冲批内跨帧复用, 每帧各自清零 (同旧版)
/// - combined 输出 map 为快照物化缓冲, 图重编译 (graphs_version 变化) 时清空 (同旧版)
///
/// `breakdown`: eval 段细分耗时出参 (纳秒累计, 观测用, 不影响行为)
pub fn process_source_batch(
    eval_state: &GraphEvalState,
    source_frames: &mut SourceFramesMap,
    source_id: &str,
    frames: &[DataFrame],
    buffer: &mut DataBuffer,
    breakdown: &mut EvalBreakdown,
) {
    use std::sync::atomic::Ordering;

    if frames.is_empty() {
        return;
    }
    let input_values = eval_state.input_values.lock().clone();
    let custom_outputs = eval_state.custom_outputs.lock().clone();
    let source_texts = eval_state.source_texts.lock();
    let graphs = eval_state.graphs.lock();
    let graphs_version = eval_state.graphs_version.load(Ordering::Relaxed);
    let mut filter_states = eval_state.filter_states.lock();
    let decoder_states = eval_state.decoder_states.lock();
    let mut ifft_states = eval_state.ifft_states.lock();
    let mut trigger_states = eval_state.trigger_states.lock();
    // analyzer 锁整批持有 (与 spectrum_ticker 同为 graphs → analyzers 顺序, 无死锁)
    let mut analyzers = eval_state.spectrum_analyzers.lock();

    // 仅保留被该源触发的图 (graph 下标固定 — 同一锁 guard 内迭代序稳定,
    // 派生边/槽位缓冲按此对齐)
    let graph_list: Vec<&CompiledGraph> = graphs
        .values()
        .filter(|g| graph_triggered_by(g, source_id))
        .collect();

    // 槽位缓冲: 每 graph 一组, 批内跨帧复用
    let mut slot_bufs: Vec<SlotBufs> = graph_list
        .iter()
        .map(|g| {
            let n = g.compiled().slot_count();
            let sn = g.compiled().str_slot_count();
            (
                vec![0.0; n],
                vec![false; n],
                vec![String::new(); sn],
                vec![false; sn],
            )
        })
        .collect();

    // 派生边预计算: (graph 下标, 槽位下标, buffer 派生索引)
    // 每批一次 (slot_of / derived_index_of 命中即返回), 逐帧零哈希直写;
    // 槽位解析不到 (图结构不含该端口) 的边本批跳过
    let mut derived_edges: Vec<(usize, usize, usize)> = Vec::new();
    for (gi, g) in graph_list.iter().enumerate() {
        for e in g.edges() {
            if let Some(slot) = g.compiled().slot_of(&e.source, &e.source_handle) {
                derived_edges.push((gi, slot, buffer.derived_index_of(&e.target, &e.source)));
            }
        }
    }

    // combined 输出 map 不再需要: 发布点直接物化进 snap.values (覆盖写语义,
    // 未触发图的旧值保留 — latest-value 融合, 取代旧版整表 swap)

    // 快照批内节流发布: 大批次 (700k 时一批可达 24ms+) 只在批尾更新快照,
    // 数值读数 (MathWidget/Gauge 等走 output_snapshot) 会明显落后波形轨迹
    // (波形是逐帧 push 进 buffer、流式 drain 的)。每 ~8ms 中途发布一次。
    const PUBLISH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(8);
    let mut last_publish = std::time::Instant::now();

    for (i, frame) in frames.iter().enumerate() {
        // 0. 该源最新帧入缓存 (其他源保持缓存值 — latest-value 融合)
        //    clone_from 复用 channels 分配, 稳态零分配
        match source_frames.get_mut(source_id) {
            Some(slot) => {
                slot.timestamp = frame.timestamp;
                slot.channels.clone_from(&frame.channels);
            }
            None => {
                source_frames.insert(source_id.to_string(), frame.clone());
            }
        }

        // 1. push 原始帧到该源自己的 buffer
        let t = std::time::Instant::now();
        buffer.push_frame(frame);
        breakdown.push_frame_ns += t.elapsed().as_nanos() as u64;

        // 2. 评估被触发的图 (编译期槽位表, 纯数组读写零字符串哈希)
        let t = std::time::Instant::now();
        for (gi, g) in graph_list.iter().enumerate() {
            let (slots, written, str_slots, str_written) = &mut slot_bufs[gi];
            // 每帧清零 (memset/clear): slots 防上帧值泄漏, written 复刻 "本帧未产出 = 键不存在"
            slots.fill(0.0);
            written.fill(false);
            str_slots.iter_mut().for_each(String::clear);
            str_written.fill(false);
            g.compiled().run(
                source_frames,
                &source_texts,
                &input_values,
                &custom_outputs,
                &mut filter_states,
                &decoder_states,
                &mut ifft_states,
                &mut trigger_states,
                slots,
                written,
                str_slots,
                str_written,
            );
        }
        breakdown.graph_eval_ns += t.elapsed().as_nanos() as u64;

        // 3. 收集派生值 (批首预计算索引, 与 push_frame 时间戳对齐; 仅 written 槽位)
        let t = std::time::Instant::now();
        for &(gi, slot, buf_idx) in &derived_edges {
            let (slots, written, ..) = &slot_bufs[gi];
            if written[slot] {
                buffer.push_derived_idx(buf_idx, slots[slot]);
            }
        }
        breakdown.derived_ns += t.elapsed().as_nanos() as u64;

        // 4. 收集 SpectrumSink 输入值, push 到对应 analyzer 的滑动窗口 (仅 written 槽位)
        let t = std::time::Instant::now();
        if !analyzers.is_empty() {
            for (gi, g) in graph_list.iter().enumerate() {
                let (slots, written, ..) = &slot_bufs[gi];
                for (sink_id, value) in g.compiled().spectrum_values(slots, written) {
                    if let Some(analyzer) = analyzers.get_mut(sink_id) {
                        analyzer.push(value);
                    }
                }
            }
        }
        breakdown.spectrum_ns += t.elapsed().as_nanos() as u64;

        // 5. 每 1024 帧检查一次, 距上次发布 ≥8ms 则中途发布快照
        //    (物化当前帧槽位直接合并进 snap.values — 覆盖写, 未触发图旧值保留)
        //    注: 快照发布 (步骤 5/6) 不计入细分耗时 (物化 + 锁, 发布点稀疏)
        if i & 0x3FF == 0x3FF && last_publish.elapsed() >= PUBLISH_INTERVAL {
            {
                let mut snap = eval_state.output_snapshot.lock();
                for (gi, g) in graph_list.iter().enumerate() {
                    let (slots, written, ..) = &slot_bufs[gi];
                    g.compiled().materialize(slots, written, &mut snap.values);
                }
                snap.tick = snap.tick.wrapping_add(1);
            }
            // 字符串输出与 f32 同点发布 (节流对齐, 仅 written 置位槽位物化)
            publish_str_slots(
                &graph_list,
                &slot_bufs,
                &mut eval_state.graph_string_outputs.lock(),
            );
            last_publish = std::time::Instant::now();
        }
    }

    // 6. 批尾最终发布 (保证批尾帧的值一定可见) —
    //    图重编译后旧快照含过期节点 → 先清空再物化, 保证过期键不回流
    //    (清空后未触发图的键暂时消失, 待其源触发或快照评估时重建 — latest-value 语义)
    let version_changed = {
        let mut snap = eval_state.output_snapshot.lock();
        let changed = snap.graphs_version != graphs_version;
        if changed {
            snap.values.clear();
            snap.graphs_version = graphs_version;
        }
        for (gi, g) in graph_list.iter().enumerate() {
            let (slots, written, ..) = &slot_bufs[gi];
            g.compiled().materialize(slots, written, &mut snap.values);
        }
        snap.tick = snap.tick.wrapping_add(1);
        changed
    };

    // 字符串输出批尾发布: 与 f32 快照同一生命周期 —
    // 图重编译时同步清空 (过期节点 id 不回流), 再覆盖写本批物化结果
    if version_changed {
        eval_state.graph_string_outputs.lock().clear();
    }
    publish_str_slots(
        &graph_list,
        &slot_bufs,
        &mut eval_state.graph_string_outputs.lock(),
    );
}
