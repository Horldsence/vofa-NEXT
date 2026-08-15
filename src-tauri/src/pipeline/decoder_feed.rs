use crate::state::GraphEvalState;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use vofa_next_nodes::FrameParser;

/// FrameDecoder 解析配置 (不含 loopback — 它只决定"谁来喂", 不影响解析行为/重建判定)
/// 元组: (blocks, enable_valid, enable_frame_count, enable_last_timestamp, enable_fps)
pub type DecoderParseConfig = (
    Vec<vofa_next_nodes::DecoderBlockDef>,
    bool,
    bool,
    bool,
    bool,
);

/// 从 graphs 收集所有 FrameDecoder 配置 → (dec_id, DecoderParseConfig, loopback)
fn collect_decoder_configs(
    eval_state: &GraphEvalState,
) -> HashMap<String, (DecoderParseConfig, bool)> {
    let graphs = eval_state.graphs.lock();
    let mut configs: HashMap<String, (DecoderParseConfig, bool)> = HashMap::new();
    for (_, graph) in graphs.iter() {
        for dec_id in graph.decoder_node_ids() {
            if let Some(cfg) = graph.decoder_config(&dec_id) {
                configs.insert(
                    dec_id,
                    (
                        (cfg.0.to_vec(), cfg.1, cfg.2, cfg.3, cfg.4),
                        cfg.5, // loopback
                    ),
                );
            }
        }
    }
    configs
}

/// 确保 decoder 的 FrameParser 存在且与配置一致 (缺失则创建, 配置变更则重建),
/// 并确保旁路收集器存在。
pub fn ensure_decoder(eval_state: &GraphEvalState, dec_id: &str, config: &DecoderParseConfig) {
    let (blocks, ev, efc, elt, efps) = config;
    let mut decoder_states = eval_state.decoder_states.lock();
    let need_rebuild = match decoder_states.get(dec_id) {
        None => true,
        Some(p) => !p.matches_config(blocks, *ev, *efc, *elt, *efps),
    };
    if need_rebuild {
        let parser = FrameParser::new(blocks.clone(), *ev, *efc, *elt, *efps);
        decoder_states.insert(dec_id.to_string(), parser);
        log::info!(
            "帧解码器已 (重新)创建: decoder={} blocks={} valid={} count={} ts={} fps={}",
            dec_id,
            blocks.len(),
            ev,
            efc,
            elt,
            efps
        );
    }
    // 确保旁路收集器存在 (Arc<Mutex<RawDataCollector>> 实现 Default, 供订阅任务共享)
    eval_state
        .decoder_raw_collectors
        .lock()
        .entry(dec_id.to_string())
        .or_default();
}

/// 确保 parser 存在并喂入字节, 更新 last_frame;
/// 每帧消费的原始字节推入该 decoder 的旁路收集器 (供前端 RawData 独立通道显示)。
///
/// 供两条路径共用:
/// - data_loop: 实时 RX 默认喂入 (feed_frame_decoders_cached, 仅非 loopback 解码器)
/// - inject_loopback_bytes: 回环边注入 (仅 loopback 解码器, 与串口开关无关)
pub fn feed_one_decoder(
    eval_state: &GraphEvalState,
    dec_id: &str,
    config: &DecoderParseConfig,
    data: &[u8],
    ts_us: u64,
) {
    ensure_decoder(eval_state, dec_id, config);

    let mut decoder_states = eval_state.decoder_states.lock();
    if let Some(parser) = decoder_states.get_mut(dec_id) {
        let parsed = parser.feed(data, ts_us);
        if !parsed.is_empty() {
            let collectors = eval_state.decoder_raw_collectors.lock();
            if let Some(collector) = collectors.get(dec_id) {
                let mut col = collector.lock();
                for frame in &parsed {
                    if !frame.raw_bytes.is_empty() {
                        col.push_chunk(ts_us, &frame.raw_bytes);
                    }
                }
            }
        }
    }
}

/// FrameDecoder 配置缓存 — 按 graphs_version 失效, 避免 feed_task 每批抢 graphs 锁
/// (eval_task 整批持有 graphs 锁, 抢锁会把两段流水线串行化)
pub struct DecoderFeedCache {
    version: u64, // u64::MAX = 未初始化
    configs: HashMap<String, (DecoderParseConfig, bool)>,
}

impl DecoderFeedCache {
    pub fn new() -> Self {
        Self {
            version: u64::MAX,
            configs: HashMap::new(),
        }
    }
}

impl Default for DecoderFeedCache {
    fn default() -> Self {
        Self::new()
    }
}

/// 同步 decoder_states 与 graphs 中的 FrameDecoder 节点, 并喂入新字节 (配置缓存版)
///
/// 步骤:
/// 1. 读取 graphs_version, 仅版本变化时才锁 graphs 重新收集 decoder 配置,
///    并删除已不存在的 decoder 对应的 parser / 旁路收集器
/// 2. 对每个缓存的 decoder:
///    - 确保 parser 存在/最新 (ensure_decoder)
///    - loopback=false: 调用 feed_one_decoder 喂入实时字节
///    - loopback=true:  跳过 — 它只接收 inject_loopback_bytes 经回环边注入的字节
///
/// 由 data_loop 在每包数据上调用; decoder 配置只随图变化,
/// 缓存避免 feed_task 每批抢 graphs 锁 (该锁被 eval_task 整批持有)。
///
/// 返回: 是否存在 FrameDecoder 节点 (含 loopback 节点,
/// 供 data_loop 决定是否在 frames 为空时仍调用 evaluate)
pub fn feed_frame_decoders_cached(
    eval_state: &GraphEvalState,
    data: &[u8],
    ts_us: u64,
    cache: &mut DecoderFeedCache,
) -> bool {
    let v = eval_state.graphs_version.load(Ordering::Relaxed);
    if cache.version != v {
        cache.configs = collect_decoder_configs(eval_state);
        cache.version = v;

        // 删除已不存在的 decoder 对应的 parser, 同步清理旁路收集器
        eval_state
            .decoder_states
            .lock()
            .retain(|id, _| cache.configs.contains_key(id));
        eval_state
            .decoder_raw_collectors
            .lock()
            .retain(|id, _| cache.configs.contains_key(id));
    }

    for (dec_id, (config, loopback)) in &cache.configs {
        if *loopback {
            // loopback 解码器不接收实时 RX, 但仍需确保 parser/collector 存在
            // (否则 retain 清理后输出无默认值, 且注入时需重建)
            ensure_decoder(eval_state, dec_id, config);
        } else {
            feed_one_decoder(eval_state, dec_id, config, data, ts_us);
        }
    }

    !cache.configs.is_empty()
}
