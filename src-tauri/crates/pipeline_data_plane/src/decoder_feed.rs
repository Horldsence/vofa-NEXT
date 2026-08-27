//! FrameDecoder 状态同步与字节喂入 — 字节来源完全由 BytePlan 中指向该节点的
//! 字节边决定 (Transport.rx / Protocol.out / CommandSender.loopbackOut 等,
//! 可能多条); 旧版 "loopback=false 默认喂全局 RX" 逻辑已删除。
//!
//! 喂入入口: [`feed_decoder_by_id`] (由字节路由在命中 FrameDecoder 下游时调用)。
//! 配置缓存 ([`DecoderFeedCache`]) 按 graphs_version 失效的模式保留:
//! 缓存放每个 Transport 读任务内, 避免每批抢 graphs 锁 (该锁被评估路径整批持有)。

use crate::eval_state::GraphEvalState;
use buffer_raw::RawDataDirection;
use node_frame_decoder::FrameParser;
use std::collections::HashMap;
use std::sync::atomic::Ordering;

/// FrameDecoder 解析配置 (blocks + 附加端口开关)
/// 元组: (blocks, enable_valid, enable_frame_count, enable_last_timestamp, enable_fps)
pub type DecoderParseConfig = (Vec<node_kind::DecoderBlockDef>, bool, bool, bool, bool);

/// 从 graphs 收集所有 FrameDecoder 配置 → (dec_id, DecoderParseConfig)
fn collect_decoder_configs(eval_state: &GraphEvalState) -> HashMap<String, DecoderParseConfig> {
    let graphs = eval_state.graphs.lock();
    let mut configs: HashMap<String, DecoderParseConfig> = HashMap::new();
    for (_, graph) in graphs.iter() {
        for dec_id in graph.decoder_node_ids() {
            if let Some(cfg) = graph.decoder_config(dec_id) {
                // cfg.5 为 deprecated 的 loopback 标志, 新语义下忽略
                // (字节来源完全由输入字节边决定)
                configs.insert(dec_id.clone(), (cfg.0.to_vec(), cfg.1, cfg.2, cfg.3, cfg.4));
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
    let need_rebuild = decoder_states
        .get(dec_id)
        .is_none_or(|p| !p.matches_config(blocks, *ev, *efc, *elt, *efps));
    if need_rebuild {
        let parser = FrameParser::new(blocks.clone(), *ev, *efc, *elt, *efps);
        decoder_states.insert(dec_id.to_string(), parser);
        log::debug!(
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
/// 由字节路由统一调用 (Transport.rx / Protocol.out / loopbackOut → FrameDecoder.in)。
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
                        col.push_chunk(ts_us, RawDataDirection::Rx, &frame.raw_bytes);
                    }
                }
            }
        }
    }
}

/// FrameDecoder 配置缓存 — 按 graphs_version 失效, 避免读任务每批抢 graphs 锁
/// (评估路径整批持有 graphs 锁, 抢锁会把字节平面与数值平面串行化)
pub struct DecoderFeedCache {
    version: u64, // u64::MAX = 未初始化
    configs: HashMap<String, DecoderParseConfig>,
}

impl DecoderFeedCache {
    pub fn new() -> Self {
        Self {
            version: u64::MAX,
            configs: HashMap::new(),
        }
    }

    /// 版本变化时重新收集配置, 并删除已不存在的 decoder 对应的 parser / 旁路收集器
    pub fn sync(&mut self, eval_state: &GraphEvalState) {
        let v = eval_state.graphs_version.load(Ordering::Relaxed);
        if self.version == v {
            return;
        }
        self.configs = collect_decoder_configs(eval_state);
        self.version = v;

        eval_state
            .decoder_states
            .lock()
            .retain(|id, _| self.configs.contains_key(id));
        eval_state
            .decoder_raw_collectors
            .lock()
            .retain(|id, _| self.configs.contains_key(id));
    }
}

impl Default for DecoderFeedCache {
    fn default() -> Self {
        Self::new()
    }
}

/// 按边路由喂入指定 FrameDecoder — 字节路由命中 FrameDecoder 下游时调用
///
/// 返回: 该 decoder 是否存在并已喂入 (调用方据此决定是否做快照评估)
pub fn feed_decoder_by_id(
    eval_state: &GraphEvalState,
    dec_id: &str,
    data: &[u8],
    ts_us: u64,
    cache: &mut DecoderFeedCache,
) -> bool {
    cache.sync(eval_state);
    let Some(config) = cache.configs.get(dec_id) else {
        return false;
    };
    feed_one_decoder(eval_state, dec_id, config, data, ts_us);
    true
}

/// 立即同步 decoder_states 与 graphs (图重编译后调用, 不依赖喂入路径)
pub fn sync_decoders_now(eval_state: &GraphEvalState) {
    let mut cache = DecoderFeedCache::new();
    cache.sync(eval_state);
}
