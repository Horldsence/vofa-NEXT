use crate::state::GraphEvalState;
use vofa_next_buffer::DataBuffer;
use vofa_next_core::DataFrame;

/// eval 段细分耗时 (纳秒累计, 由调用方汇入 PipelineMetrics)
#[derive(Default)]
pub struct EvalBreakdown {
    pub push_frame_ns: u64,
    pub graph_eval_ns: u64,
    pub derived_ns: u64,
    pub spectrum_ns: u64,
}

/// 评估所有图 (静态函数版本, 供 GraphEvalState 使用)
///
/// 步骤:
/// 1. 对每个图调用 evaluate (传入 filter_states + decoder_states, 逐点滤波/解码跨帧持久化)
/// 2. 合并所有图输出到 output_snapshot
/// 3. 遍历所有图的 SpectrumSink, 从 output_snapshot 取输入值, push 到对应 analyzer
pub fn evaluate_all_graphs_with(eval_state: &GraphEvalState, frame: &DataFrame) {
    let input_values = eval_state.input_values.lock().clone();
    let custom_outputs = eval_state.custom_outputs.lock().clone();
    let graphs = eval_state.graphs.lock();
    let mut filter_states = eval_state.filter_states.lock();
    let decoder_states = eval_state.decoder_states.lock();

    let mut combined: vofa_next_nodes::ValuesMap = Default::default();
    for (_, graph) in graphs.iter() {
        let out = graph.evaluate(
            frame,
            &input_values,
            &custom_outputs,
            &mut filter_states,
            &decoder_states,
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

/// 批处理版帧处理 — 一个 RX 包的所有帧一次性完成 push_frame + 图评估 + 派生值收集
///
/// 与逐帧调用 evaluate_all_graphs_with + 派生收集语义完全等价
/// (每帧: push_frame → evaluate → push_derived, 保证时间戳对齐), 但:
/// - input_values / custom_outputs 的锁与 clone 每包只做一次
/// - graphs / filter_states / decoder_states 的锁每包只拿一次
/// - buffer 由调用方在同一次 lock() 内传入, 不再每帧重复加锁
/// - 图评估走编译期槽位表 (CompiledEval::run): 逐帧纯数组读写, 零字符串哈希;
///   slots/written 缓冲批内跨帧复用, 每帧各自清零 (slots 防上帧值泄漏,
///   written 复刻 "本帧未产出 = 键不存在")
/// - combined 输出 map 保留为快照物化缓冲 (仅发布点 materialize);
///   图重编译 (graphs_version 变化) 时清空重建, 避免过期节点残留
/// - 派生边每批预计算一次 (graph 下标, 槽位下标, buffer 索引), 逐帧零哈希直写
/// - snapshot.values 用 swap 交换 (旧 map 成为下一次物化的复用缓冲, 零深拷贝)
///
/// `breakdown`: eval 段细分耗时出参 (纳秒累计, 观测用, 不影响行为)
pub fn process_frames_batch(
    eval_state: &GraphEvalState,
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
    let graphs = eval_state.graphs.lock();
    let graphs_version = eval_state.graphs_version.load(Ordering::Relaxed);
    let mut filter_states = eval_state.filter_states.lock();
    let decoder_states = eval_state.decoder_states.lock();
    // analyzer 锁整批持有 (与 spectrum_ticker 同为 graphs → analyzers 顺序, 无死锁)
    let mut analyzers = eval_state.spectrum_analyzers.lock();

    // graph 下标固定 (同一锁 guard 内迭代序稳定), 派生边/槽位缓冲按此对齐
    let graph_list: Vec<&vofa_next_nodes::CompiledGraph> = graphs.values().collect();

    // 槽位缓冲: 每 graph 一组 (slots, written), 批内跨帧复用
    let mut slot_bufs: Vec<(Vec<f32>, Vec<bool>)> = graph_list
        .iter()
        .map(|g| {
            let n = g.compiled().slot_count();
            (vec![0.0; n], vec![false; n])
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

    // combined 保留为快照物化缓冲: 仅发布点由 materialize 覆盖写;
    // snapshot swap 换出的旧 map 成为下一次物化的复用缓冲, 稳态零分配
    let mut combined: vofa_next_nodes::ValuesMap = Default::default();

    // 快照批内节流发布: 大批次 (700k 时一批可达 24ms+) 只在批尾更新快照,
    // 数值读数 (MathWidget/Gauge 等走 output_snapshot) 会明显落后波形轨迹
    // (波形是逐帧 push 进 buffer、流式 drain 的)。每 ~8ms 中途发布一次。
    const PUBLISH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(8);
    let mut last_publish = std::time::Instant::now();

    for (i, frame) in frames.iter().enumerate() {
        // 1. push 原始帧到 buffer
        let t = std::time::Instant::now();
        buffer.push_frame(frame);
        breakdown.push_frame_ns += t.elapsed().as_nanos() as u64;

        // 2. 评估所有图 (编译期槽位表, 纯数组读写零字符串哈希)
        let t = std::time::Instant::now();
        for (gi, g) in graph_list.iter().enumerate() {
            let (slots, written) = &mut slot_bufs[gi];
            // 每帧清零 (memset): slots 防上帧值泄漏, written 复刻 "本帧未产出 = 键不存在"
            slots.fill(0.0);
            written.fill(false);
            g.compiled().run(
                frame,
                &input_values,
                &custom_outputs,
                &mut filter_states,
                &decoder_states,
                slots,
                written,
            );
        }
        breakdown.graph_eval_ns += t.elapsed().as_nanos() as u64;

        // 3. 收集派生值 (批首预计算索引, 与 push_frame 时间戳对齐; 仅 written 槽位)
        let t = std::time::Instant::now();
        for &(gi, slot, buf_idx) in &derived_edges {
            let (slots, written) = &slot_bufs[gi];
            if written[slot] {
                buffer.push_derived_idx(buf_idx, slots[slot]);
            }
        }
        breakdown.derived_ns += t.elapsed().as_nanos() as u64;

        // 4. 收集 SpectrumSink 输入值, push 到对应 analyzer 的滑动窗口 (仅 written 槽位)
        let t = std::time::Instant::now();
        if !analyzers.is_empty() {
            for (gi, g) in graph_list.iter().enumerate() {
                let (slots, written) = &slot_bufs[gi];
                for (sink_id, value) in g.compiled().spectrum_values(slots, written) {
                    if let Some(analyzer) = analyzers.get_mut(sink_id) {
                        analyzer.push(value);
                    }
                }
            }
        }
        breakdown.spectrum_ns += t.elapsed().as_nanos() as u64;

        // 5. 每 1024 帧检查一次, 距上次发布 ≥8ms 则中途发布快照
        //    (物化当前帧槽位到 combined 再 swap, 语义与批尾发布一致)
        //    注: 快照发布 (步骤 5/6) 不计入细分耗时 (物化 + 锁 + swap, 发布点稀疏)
        if i & 0x3FF == 0x3FF && last_publish.elapsed() >= PUBLISH_INTERVAL {
            for (gi, g) in graph_list.iter().enumerate() {
                let (slots, written) = &slot_bufs[gi];
                g.compiled().materialize(slots, written, &mut combined);
            }
            let mut snap = eval_state.output_snapshot.lock();
            snap.tick = snap.tick.wrapping_add(1);
            std::mem::swap(&mut snap.values, &mut combined);
            last_publish = std::time::Instant::now();
        }
    }

    // 6. 批尾最终发布 (保证批尾帧的值一定可见) —
    //    图重编译后旧快照含过期节点 → 先清空再物化 + swap, 保证过期键不回流
    for (gi, g) in graph_list.iter().enumerate() {
        let (slots, written) = &slot_bufs[gi];
        g.compiled().materialize(slots, written, &mut combined);
    }
    let mut snap = eval_state.output_snapshot.lock();
    if snap.graphs_version != graphs_version {
        snap.values.clear();
        snap.graphs_version = graphs_version;
    }
    snap.tick = snap.tick.wrapping_add(1);
    std::mem::swap(&mut snap.values, &mut combined);
}
