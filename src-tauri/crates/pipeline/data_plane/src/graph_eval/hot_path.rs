//! 单源帧批处理热路径 — 700k fps 主循环, 移动时不得改动任何函数体

use std::collections::HashMap;
use std::hash::BuildHasher;
use std::sync::Arc;

use buffer_databuffer::DerivedWriter;
use data_bus::TopicKey;
use engine::{CompiledGraph, SourceFramesMap};
use kind::NodeKind;
use vofa_core::DataFrame;

use crate::eval_state::GraphEvalState;

use super::predicates::{graph_requires_full_batch, graph_triggered_by, records_waveform_history};

/// 分段计时抽样周期 — Instant::now 本身在 700k fps 下占 5-7% 纯观测开销,
/// 每 64 帧抽 1 帧计时 ×64 估算分段耗时 (观测用, 不影响行为)
const TIMING_SAMPLE_PERIOD: u64 = 64;

/// eval 段细分耗时 (纳秒累计, 由调用方汇入数据平面指标)
#[derive(Default)]
pub struct EvalBreakdown {
    pub push_frame_ns: u64,
    pub graph_eval_ns: u64,
    pub derived_ns: u64,
    pub spectrum_ns: u64,
}

/// 每 graph 一组槽位缓冲 (slots, written, str_slots, str_written), 批内跨帧复用
pub type SlotBufs = (Vec<f32>, Vec<bool>, Vec<String>, Vec<bool>);

struct PortSampleBatch {
    key: TopicKey,
    graph_index: usize,
    slot: usize,
    timestamps: Vec<u64>,
    values: Vec<f64>,
}

/// StringValuesMap (FxHash) 深合并进快照 map (std hasher) — 移动语义, 字符串零 clone
///
/// 同 (node, port) 覆盖写; 两侧 hasher 不同 (FxHash vs SipHash) 故逐条目迁移
// dst 是快照侧 std-hasher map (graph_string_outputs 的值域), 内层不做 hasher 泛型
#[allow(clippy::implicit_hasher)]
pub fn merge_str_map<S: BuildHasher>(
    src: engine::StringValuesMap,
    dst: &mut HashMap<String, HashMap<String, String>, S>,
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
    static_list: &[&CompiledGraph],
    static_bufs: &[SlotBufs],
    out: &mut HashMap<String, HashMap<String, String>>,
) {
    let mut buf = engine::StringValuesMap::default();
    for (gi, g) in graph_list.iter().enumerate() {
        let (_, _, str_slots, str_written) = &slot_bufs[gi];
        g.compiled()
            .materialize_str(str_slots, str_written, &mut buf);
    }
    for (bufs, g) in static_bufs.iter().zip(static_list) {
        g.compiled().materialize_str(&bufs.2, &bufs.3, &mut buf);
    }
    merge_str_map(buf, out);
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
    derived: &DerivedWriter,
    breakdown: &mut EvalBreakdown,
) {
    use std::sync::atomic::Ordering;

    if frames.is_empty() {
        return;
    }
    let input_values = eval_state.input_values.read();
    let custom_outputs = eval_state.custom_outputs.read();
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
    // 派生边/槽位缓冲按此对齐); 静态纯本地图 (输入批内不变的纯函数图) 单独
    // 收集 — 每批评估一次, 输出值与逐帧评估相同 (见 is_static_local)
    let mut graph_list: Vec<&CompiledGraph> = Vec::new();
    let mut static_list: Vec<&CompiledGraph> = Vec::new();
    for g in graphs.values() {
        if !graph_triggered_by(g, source_id) {
            continue;
        }
        if g.compiled().is_static_local() {
            static_list.push(g);
        } else {
            graph_list.push(g);
        }
    }

    // 判定与求值共用同一图锁，防止热重编译在判定后加入有状态算子而漏帧。
    let frames = if !static_list.is_empty()
        || graph_list
            .iter()
            .any(|graph| graph_requires_full_batch(graph))
    {
        frames
    } else {
        &frames[frames.len() - 1..]
    };

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

    // 只为存在订阅的端口建立批次，未订阅端口保持热路径零额外分配。
    let mut port_batches = Vec::<PortSampleBatch>::new();
    for (graph_index, graph) in graph_list.iter().enumerate() {
        for (slot, (node_id, port)) in graph.compiled().slot_names().iter().enumerate() {
            // ProtocolSource 已由帧分发按源发布；这里再次发布会让 RawData 每帧重复两次。
            if matches!(
                graph.value_def(node_id).map(|node| &node.kind),
                Some(NodeKind::ProtocolSource { .. })
            ) {
                continue;
            }
            let key = TopicKey::new(node_id, port);
            if eval_state.data_bus.is_active(&key) {
                port_batches.push(PortSampleBatch {
                    key,
                    graph_index,
                    slot,
                    timestamps: Vec::with_capacity(frames.len()),
                    values: Vec::with_capacity(frames.len()),
                });
            }
        }
    }

    // 派生边预计算: (graph 下标, 槽位下标, buffer 派生索引)
    // 每批一次 (slot_of / derived_port_index_of 命中即返回), 逐帧零哈希直写;
    // 槽位解析不到 (图结构不含该端口) 的边本批跳过
    let mut derived_edges: Vec<(usize, usize, usize)> = Vec::new();
    for (gi, g) in graph_list.iter().enumerate() {
        for e in g.edges().filter(|edge| records_waveform_history(g, edge)) {
            if let Some(slot) = g.compiled().slot_of(&e.source, &e.source_handle) {
                derived_edges.push((
                    gi,
                    slot,
                    derived.port_index_of(&e.target, &e.source, &e.source_handle),
                ));
            }
        }
    }

    // 静态图: 每批评估一次 (无状态节点 — 全表传参无副作用), 派生边批首预计算
    let mut static_bufs: Vec<SlotBufs> = static_list
        .iter()
        .map(|g| {
            let c = g.compiled();
            (
                vec![0.0; c.slot_count()],
                vec![false; c.slot_count()],
                vec![String::new(); c.str_slot_count()],
                vec![false; c.str_slot_count()],
            )
        })
        .collect();
    let t_static = std::time::Instant::now();
    for (bufs, g) in static_bufs.iter_mut().zip(&static_list) {
        g.compiled().run(
            source_frames,
            &source_texts,
            &input_values,
            &custom_outputs,
            &mut filter_states,
            &decoder_states,
            &mut ifft_states,
            &mut trigger_states,
            &mut bufs.0,
            &mut bufs.1,
            &mut bufs.2,
            &mut bufs.3,
        );
    }
    breakdown.graph_eval_ns += u64::try_from(t_static.elapsed().as_nanos()).unwrap_or(u64::MAX);

    let mut static_edges: Vec<Vec<(usize, usize)>> = vec![Vec::new(); static_list.len()];
    for (g, edges) in static_list.iter().zip(&mut static_edges) {
        for e in g.edges().filter(|edge| records_waveform_history(g, edge)) {
            if let Some(slot) = g.compiled().slot_of(&e.source, &e.source_handle) {
                edges.push((
                    slot,
                    derived.port_index_of(&e.target, &e.source, &e.source_handle),
                ));
            }
        }
    }

    // combined 输出 map 不再需要: 发布点直接物化进 snap.values (覆盖写语义,
    // 未触发图的旧值保留 — latest-value 融合, 取代旧版整表 swap)

    // 快照批内节流发布: 大批次 (700k 时一批可达 24ms+) 只在批尾更新快照,
    // 数值读数 (MathWidget/Gauge 等走 output_snapshot) 会明显落后波形轨迹
    // (波形是逐帧 push 进 buffer、流式 drain 的)。每 ~8ms 中途发布一次。
    let publish_interval = std::time::Duration::from_millis(8);
    let mut last_publish = std::time::Instant::now();

    // 帧时间戳由字节平面采样时钟权威给定 (单一时钟域, 见 ProtocolNodeState::
    // restamp_frames_at_rate); 数值平面不再做任何时间戳加工 — 到达节奏绝不参与
    // 显示时间轴。
    let mut staged_derived = Vec::new();
    for (i, frame) in frames.iter().enumerate() {
        let timing_sampled = u64::try_from(i).unwrap_or(0) % TIMING_SAMPLE_PERIOD == 0;
        let frame_ts = frame.timestamp;
        // 0. 该源最新帧入缓存 (其他源保持缓存值 — latest-value 融合)
        //    clone_from 复用 channels 分配, 稳态零分配
        match source_frames.get_mut(source_id) {
            Some(slot) => {
                slot.timestamp = frame_ts;
                slot.channels.clone_from(&frame.channels);
            }
            None => {
                let mut owned = frame.clone();
                owned.timestamp = frame_ts;
                source_frames.insert(source_id.to_string(), owned);
            }
        }

        // 2. 评估被触发的图 (编译期槽位表, 纯数组读写零字符串哈希)
        //    (原始帧入库已由记录平面 record_frames 完成, 本层只做求值)
        let t = if timing_sampled {
            Some(std::time::Instant::now())
        } else {
            None
        };
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
        if let Some(t) = t {
            breakdown.graph_eval_ns +=
                u64::try_from(t.elapsed().as_nanos()).unwrap_or(u64::MAX) * TIMING_SAMPLE_PERIOD;
        }

        // 只有 written=true 的槽位才进入端口历史。缺源/越界不会生成假样本。
        for batch in &mut port_batches {
            let (slots, written, ..) = &slot_bufs[batch.graph_index];
            if written[batch.slot] {
                batch.timestamps.push(frame_ts);
                batch.values.push(f64::from(slots[batch.slot]));
            }
        }

        // 3. 收集派生值 (批首预计算索引, 携带帧时间戳写派生独立时间轴; 仅 written 槽位)
        let t = if timing_sampled {
            Some(std::time::Instant::now())
        } else {
            None
        };
        for &(gi, slot, buf_idx) in &derived_edges {
            let (slots, written, ..) = &slot_bufs[gi];
            if written[slot] {
                staged_derived.push((buf_idx, frame_ts, slots[slot]));
            }
        }
        // 静态图派生值逐帧重复 push (输出批内不变 — 常值重复写入与逐帧评估等价,
        // 且保持派生序列对窗口的时间覆盖)
        for (gi, edges) in static_edges.iter().enumerate() {
            let (slots, written, ..) = &static_bufs[gi];
            for (slot, buf_idx) in edges {
                if written[*slot] {
                    staged_derived.push((*buf_idx, frame_ts, slots[*slot]));
                }
            }
        }
        if let Some(t) = t {
            breakdown.derived_ns +=
                u64::try_from(t.elapsed().as_nanos()).unwrap_or(u64::MAX) * TIMING_SAMPLE_PERIOD;
        }

        // 4. 收集 Fft 输入值, push 到对应 analyzer 的滑动窗口 (仅 written 槽位)
        let t = if timing_sampled {
            Some(std::time::Instant::now())
        } else {
            None
        };
        if !analyzers.is_empty() {
            for (gi, g) in graph_list.iter().enumerate() {
                let (slots, written, ..) = &slot_bufs[gi];
                for (sink_id, value) in g.compiled().spectrum_values(slots, written) {
                    if let Some(analyzer) = analyzers.get_mut(sink_id) {
                        analyzer.push_with(value, |frame| {
                            for target in g.spectrum_consumers(sink_id) {
                                if let Err(error) =
                                    ifft_states.entry(target.clone()).or_default().accept(frame)
                                {
                                    log::warn!("IFFT {target}: {error}");
                                }
                            }
                        });
                    }
                }
            }
        }
        if let Some(t) = t {
            breakdown.spectrum_ns +=
                u64::try_from(t.elapsed().as_nanos()).unwrap_or(u64::MAX) * TIMING_SAMPLE_PERIOD;
        }

        // 5. 每 1024 帧检查一次, 距上次发布 ≥8ms 则中途发布快照
        //    (物化当前帧槽位直接合并进 snap.values — 覆盖写, 未触发图旧值保留)
        //    注: 快照发布 (步骤 5/6) 不计入细分耗时 (物化 + 锁, 发布点稀疏)
        if i & 0x3FF == 0x3FF && last_publish.elapsed() >= publish_interval {
            {
                let mut snap = eval_state.output_snapshot.lock();
                for (gi, g) in graph_list.iter().enumerate() {
                    let (slots, written, ..) = &slot_bufs[gi];
                    g.compiled().materialize(slots, written, &mut snap.values);
                }
                for (bufs, g) in static_bufs.iter().zip(&static_list) {
                    g.compiled().materialize(&bufs.0, &bufs.1, &mut snap.values);
                }
                snap.tick = snap.tick.wrapping_add(1);
            }
            // 字符串输出与 f32 同点发布 (节流对齐, 仅 written 置位槽位物化)
            publish_str_slots(
                &graph_list,
                &slot_bufs,
                &static_list,
                &static_bufs,
                &mut eval_state.graph_string_outputs.lock(),
            );
            last_publish = std::time::Instant::now();
        }
    }

    // 单批只获取一次派生锁；原始 DataBuffer 外层锁从未参与本循环。
    derived.append(staged_derived);

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
        for (bufs, g) in static_bufs.iter().zip(&static_list) {
            g.compiled().materialize(&bufs.0, &bufs.1, &mut snap.values);
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
        &static_list,
        &static_bufs,
        &mut eval_state.graph_string_outputs.lock(),
    );

    // 静态图端口批: 批尾单样本 (批尾帧时间戳) — 批内输出常量, 每帧重复发布
    // 是纯浪费 (样本密度 delta 经等价性测试单测锚定);
    // 锁释放前收集, 与动态批一起发布
    let mut static_publish: Vec<(TopicKey, u64, f64)> = Vec::new();
    if let Some(last) = frames.last() {
        for (bufs, g) in static_bufs.iter().zip(&static_list) {
            let compiled = g.compiled();
            for (slot, (node_id, port)) in compiled.slot_names().iter().enumerate() {
                if matches!(
                    g.value_def(node_id).map(|node| &node.kind),
                    Some(NodeKind::ProtocolSource { .. })
                ) {
                    continue;
                }
                let key = TopicKey::new(node_id, port);
                if eval_state.data_bus.is_active(&key) && bufs.1[slot] {
                    static_publish.push((key, last.timestamp, f64::from(bufs.0[slot])));
                }
            }
        }
    }

    drop(analyzers);
    drop(trigger_states);
    drop(ifft_states);
    drop(decoder_states);
    drop(filter_states);
    drop(graphs);
    drop(source_texts);

    for batch in port_batches {
        if !batch.values.is_empty() {
            eval_state.data_bus.publish_samples(
                batch.key,
                Arc::from(batch.timestamps),
                Arc::from(batch.values),
            );
        }
    }
    for (key, ts, value) in static_publish {
        eval_state
            .data_bus
            .publish_samples(key, Arc::from([ts]), Arc::from([value]));
    }
}
