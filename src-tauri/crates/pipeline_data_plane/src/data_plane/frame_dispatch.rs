//! 帧分发 — Protocol 节点产帧 → source_frames 缓存 + 数值平面触发
//!
//! `source_frames` 是两平面衔接点: 字节平面每源最新帧缓存 (key = Protocol 节点 id,
//! latest-value 融合), 数值平面 ProtocolSource 节点求值时按源读取
//! (CompiledOp::ProtocolSource, 见 node_engine)。
//!
//! 字符串平面有对称的衔接点 `source_texts`: RawData 协议不产帧, 其原始字节经
//! [`cache_source_text`] 写入文本缓存, 供 ProtocolSource 的 "str" 端口读取。
//!
//! 触发规则 (见 [`crate::pipeline::graph_eval::process_source_batch`]):
//! 某源来帧 → 评估"引用了该源的 tab 图"与"无 ProtocolSource 的纯本地图"
//! (后者沿用旧行为: 单源时代任意来帧都评估); 同 tab 多源时其他源用缓存最新帧。

use buffer_databuffer::DataBuffer;
use node_kind::{NodeDef, NodeKind};
use parking_lot::Mutex;
use pipeline_bus::{DataBus, SampleStatus, TopicKey};
use std::collections::HashMap;
use std::sync::Arc;
use vofa_core::DataFrame;

use super::DataPlaneState;
use crate::eval_state::GraphEvalState;
use crate::graph_eval::{evaluate_snapshot_now, process_source_batch, EvalBreakdown};

/// Protocol 节点产出一批帧: 逐帧更新 source_frames → push 到该源自己的 DataBuffer →
/// 评估被该源触发的 tab 图 → 派生边回写到该源 buffer。
///
/// 同步包装 (测试 / 非 tokio 调用方): 热路径请用 [`on_frames_detached`] +
/// `spawn_blocking`, 避免重型评估占用 tokio worker。
///
/// 返回数值平面耗时 ns (push_frame + 图评估 + 派生 + 频谱, 观测用)。
pub fn on_frames(plane: &DataPlaneState, source_id: &str, frames: &[DataFrame]) -> u64 {
    let buffer = plane.buffer_for(source_id);
    let eval_workers = plane.pipeline_config.read().eval_workers;
    on_frames_detached(
        &plane.eval,
        &plane.global_nodes,
        &buffer,
        source_id,
        frames,
        eval_workers,
    )
}

/// 帧分发主体 — 只依赖 eval 状态 + 全局节点表 + 该源 buffer (全部 Arc 可克隆)
///
/// 供字节路由把整段重型同步评估丢进 `tokio::task::spawn_blocking` 执行:
/// 大批次 (700k 时可达 24ms+) 不再占住 tokio worker, await 侧保持同源批序。
///
/// `eval_workers ≥ 2` 时走图内路径分块 fork-join 并行评估
/// ([`crate::graph_eval_parallel`]), 1 = 串行热路径。
#[allow(clippy::implicit_hasher)] // 与 DataPlaneState.global_nodes 的具体 hasher 类型耦合, 泛化 S 会传染整个状态图
pub fn on_frames_detached(
    eval: &GraphEvalState,
    global_nodes: &Mutex<HashMap<String, NodeDef>>,
    buffer: &Arc<Mutex<DataBuffer>>,
    source_id: &str,
    frames: &[DataFrame],
    eval_workers: usize,
) -> u64 {
    if frames.is_empty() {
        return 0;
    }
    publish_protocol_samples(&eval.data_bus, global_nodes, source_id, frames);
    let mut buf = buffer.lock();
    let mut sf = eval.source_frames.lock();
    let mut breakdown = EvalBreakdown::default();
    if eval_workers > 1 {
        crate::graph_eval_parallel::process_source_batch_parallel(
            eval,
            &mut sf,
            source_id,
            frames,
            &mut buf,
            eval_workers,
            &mut breakdown,
        );
    } else {
        process_source_batch(eval, &mut sf, source_id, frames, &mut buf, &mut breakdown);
    }
    breakdown.push_frame_ns + breakdown.graph_eval_ns + breakdown.derived_ns + breakdown.spectrum_ns
}

/// 把协议帧按真实端口写入 Topic。只有帧中实际存在的通道才产生样本。
fn publish_protocol_samples(
    data_bus: &DataBus,
    global_nodes: &Mutex<HashMap<String, NodeDef>>,
    source_id: &str,
    frames: &[DataFrame],
) {
    let configured_names = global_nodes
        .lock()
        .get(source_id)
        .and_then(|node| match &node.kind {
            NodeKind::Protocol { schema, .. } => schema
                .as_ref()
                .map(schema_types::ProtocolSchema::port_names),
            _ => None,
        })
        .unwrap_or_default();
    let channel_count = frames
        .iter()
        .map(|frame| frame.channels.len())
        .max()
        .unwrap_or(0);

    for key in data_bus.active_topics_for_source(source_id) {
        let requested = configured_names
            .iter()
            .position(|name| name == &key.source_handle)
            .or_else(|| key.source_handle.strip_prefix("ch")?.parse::<usize>().ok());
        if let Some(requested) = requested.filter(|requested| *requested >= channel_count) {
            data_bus.set_status(
                key,
                SampleStatus::ChannelOutOfRange {
                    requested,
                    available: channel_count,
                },
            );
        }
    }

    for channel in 0..channel_count {
        let handle = configured_names
            .get(channel)
            .filter(|name| !name.is_empty())
            .cloned()
            .unwrap_or_else(|| format!("ch{channel}"));
        let key = TopicKey::new(source_id, handle);
        if !data_bus.is_active(&key) {
            continue;
        }
        let mut timestamps = Vec::with_capacity(frames.len());
        let mut values = Vec::with_capacity(frames.len());
        for frame in frames {
            if let Some(value) = frame.channels.get(channel) {
                timestamps.push(frame.timestamp);
                values.push(f64::from(*value));
            }
        }
        if values.is_empty() {
            data_bus.set_status(
                key,
                SampleStatus::ChannelOutOfRange {
                    requested: channel,
                    available: channel_count,
                },
            );
        } else {
            data_bus.publish_samples(key, Arc::from(timestamps), Arc::from(values));
        }
    }
}

/// RawData 协议原始字节 → 每源最新文本缓存 ([`super::DataPlaneState::source_texts`]) —
/// 值平面字符串端口的正式数据源
///
/// 语义与 [`on_frames`] 对称: UTF-8 lossy 解码 + latest-value 覆盖写 (空批次按空文本
/// 覆盖, 保持既有行为)。ProtocolSource 的 "str" 端口 (String 域) 求值时按源读取,
/// 无缓存时槽位不写、快照保持上次值 (见 node_eval)。
pub fn cache_source_text(plane: &DataPlaneState, source_id: &str, data: &[u8]) {
    let text = String::from_utf8_lossy(data);
    plane
        .source_texts
        .lock()
        .insert(source_id.to_string(), text.into_owned());
}

/// 快照刷新 — 字节事件 (FrameDecoder 喂入) / 输入事件 (set_input_value 等) 之后,
/// 以 source_frames 现状对所有 tab 图做一次评估并发布 output_snapshot。
///
/// 取代旧 force_eval 空帧机制: ProtocolSource 从缓存读最新值, 不再被空帧清零;
/// FrameDecoder 输出来自 decoder_states 的 last_frame 缓存。
pub fn refresh_snapshot(plane: &DataPlaneState) {
    // 克隆小 map 后即释放锁, 避免与 process_source_batch 的锁序交织
    let sf = plane.eval.source_frames.lock().clone();
    evaluate_snapshot_now(&plane.eval, &sf);
}
