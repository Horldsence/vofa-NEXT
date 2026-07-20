use crate::core::state::GraphEvalState;
use std::collections::HashMap;
use vofa_next_nodes::FrameParser;

/// 同步 decoder_states 与 graphs 中的 FrameDecoder 节点, 并喂入新字节
///
/// 步骤:
/// 1. 收集所有 graph 中当前的 FrameDecoder id → config (blocks + 附加端口开关)
/// 2. 删除已不存在的 decoder 对应的 parser
/// 3. 对每个 decoder:
///    - 若 parser 不存在 → 按当前 config 创建
///    - 若 parser 存在但 config 变了 → 重建
///    - 调用 parser.feed(data, ts) 喂入字节, 更新 last_frame
///
/// 由 data_loop 在每包数据上调用, 保证 parser 与图拓扑一致。
///
/// 返回: 是否存在 FrameDecoder 节点 (供 data_loop 决定是否在 frames 为空时仍调用 evaluate)
pub fn feed_frame_decoders(eval_state: &GraphEvalState, data: &[u8], ts_us: u64) -> bool {
    let graphs = eval_state.graphs.lock();
    let mut decoder_states = eval_state.decoder_states.lock();

    // 收集所有 graph 中当前的 FrameDecoder id → config
    let mut current_configs: HashMap<String, (Vec<vofa_next_nodes::DecoderBlockDef>, bool, bool, bool, bool)> = HashMap::new();
    for (_, graph) in graphs.iter() {
        for dec_id in graph.decoder_node_ids() {
            if let Some(cfg) = graph.decoder_config(&dec_id) {
                current_configs.insert(dec_id, (
                    cfg.0.to_vec(),
                    cfg.1, cfg.2, cfg.3, cfg.4,
                ));
            }
        }
    }

    // 删除已不存在的 decoder 对应的 parser
    decoder_states.retain(|id, _| current_configs.contains_key(id));

    // 新建/重建 parser, 并喂入字节
    for (dec_id, (blocks, ev, efc, elt, efps)) in &current_configs {
        let need_rebuild = match decoder_states.get(dec_id) {
            None => true,
            Some(p) => !p.matches_config(blocks, *ev, *efc, *elt, *efps),
        };
        if need_rebuild {
            let parser = FrameParser::new(
                blocks.clone(),
                *ev, *efc, *elt, *efps,
            );
            decoder_states.insert(dec_id.clone(), parser);
            tracing::info!(
                "帧解码器已 (重新)创建: decoder={} blocks={} valid={} count={} ts={} fps={}",
                dec_id, blocks.len(), ev, efc, elt, efps
            );
        }
        // 喂入字节 (无论是否重建, 都要喂 — 重建后 buf 为空, 直接从新数据开始解析)
        if let Some(parser) = decoder_states.get_mut(dec_id) {
            parser.feed(data, ts_us);
        }
    }

    !current_configs.is_empty()
}
