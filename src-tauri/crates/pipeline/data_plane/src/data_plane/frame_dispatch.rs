//! 帧分发 — Protocol 节点产帧 → source_frames 缓存 + 数值平面触发
//!
//! `source_frames` 是两平面衔接点: 字节平面每源最新帧缓存 (key = Protocol 节点 id,
//! latest-value 融合), 数值平面 ProtocolSource 节点求值时按源读取
//! (CompiledOp::ProtocolSource, 见 engine)。
//!
//! 字符串平面有对称的衔接点 `source_texts`: RawData 协议不产帧, 其原始字节经
//! [`cache_source_text`] 写入文本缓存, 供 ProtocolSource 的 "str" 端口读取。
//!
//! 触发规则 (见 [`crate::pipeline::graph_eval::process_source_batch`]):
//! 某源来帧 → 评估"引用了该源的 tab 图"与"无 ProtocolSource 的纯本地图"
//! (后者沿用旧行为: 单源时代任意来帧都评估); 同 tab 多源时其他源用缓存最新帧。

use buffer_databuffer::DataBuffer;
use data_bus::{DataBus, SampleStatus, TopicKey};
use kind::{NodeDef, NodeKind};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use vofa_core::{DataFrame, PipelineConfig};

use super::DataPlaneState;
use crate::eval_state::GraphEvalState;
use crate::graph_eval::{evaluate_snapshot_now, process_source_batch, EvalBreakdown};

/// 评估分派选项 — [`PipelineConfig`] 的热路径投影 (每批读取一次)
#[derive(Debug, Clone, Copy)]
pub struct EvalOptions {
    /// 数值平面评估 worker 数 (CPU 路径; 1 = 串行)
    pub workers: usize,
    /// fork-join 并行路径内 Math 单元的 SIMD 批量求值开关
    pub simd: bool,
}

impl EvalOptions {
    /// 从流水线配置投影
    #[must_use]
    pub const fn from_config(cfg: &PipelineConfig) -> Self {
        Self {
            workers: cfg.eval_workers,
            simd: cfg.eval_simd,
        }
    }
}

/// 记录平面分块大小 — 分块持锁推送, 让显示读锁可以穿插
/// (单批 14 万帧 → ~9 次锁获取, 每次 ~1ms, 不再长时间霸占缓冲锁)
const RECORD_CHUNK: usize = 16_384;

/// 端口预览发布预算 — 每批每通道最多发布的样本数 (超限步长抽点, 恒含批尾
/// 最新值)。预览面板只需走势, 全量拷贝在 10M 帧/s 下不可行, 也是预览广播
/// 溢出 ("样本预览跳过") 的压力来源。
const PREVIEW_MAX_SAMPLES_PER_BATCH: usize = 512;

/// 记录平面入口 — 原始帧**无条件**入库 (不变量 3)
///
/// 字节平面解析 + 采样时钟定案后调用: 原始通道按权威时间戳写入该源
/// DataBuffer, 分块持锁; 求值积压/丢弃不影响本路径, 波形显示因此独立于
/// 求值吞吐。端口预览发布 (降载后) 同属记录平面。
pub fn record_frames(plane: &DataPlaneState, source_id: &str, frames: &[DataFrame]) {
    if frames.is_empty() {
        return;
    }
    publish_protocol_samples(&plane.eval.data_bus, &plane.global_nodes, source_id, frames);
    let buffer = plane.buffer_for(source_id);
    for chunk in frames.chunks(RECORD_CHUNK) {
        let mut buf = buffer.lock();
        for frame in chunk {
            buf.push_frame_at(frame.timestamp, &frame.channels);
        }
    }
}

/// 求值平面入口 — source_frames 更新 + 图评估 + 派生通道写独立时间轴
///
/// eval worker 消费评估队列时调用 (记录平面已在路由时入库, 本函数不再推送
/// 原始帧)。返回数值平面耗时 ns (观测用)。
#[allow(clippy::implicit_hasher)] // 与 DataPlaneState.global_nodes 的具体 hasher 类型耦合, 泛化 S 会传染整个状态图
pub fn eval_frames(
    eval: &GraphEvalState,
    _global_nodes: &Mutex<HashMap<String, NodeDef>>,
    buffer: &Arc<Mutex<DataBuffer>>,
    source_id: &str,
    frames: &[DataFrame],
    options: EvalOptions,
) -> u64 {
    if frames.is_empty() {
        return 0;
    }
    let started = std::time::Instant::now();
    // 只在取得轻量派生写句柄时短暂锁原始缓冲。图求值随后只锁独立的
    // DerivedStore，绝不能在整批期间占住记录平面的 DataBuffer 外层锁。
    let derived = buffer.lock().derived_writer();
    let mut sf = eval.source_frames.lock();
    let mut breakdown = EvalBreakdown::default();

    if options.workers > 1 {
        crate::graph_eval_parallel::process_source_batch_parallel(
            eval,
            &mut sf,
            source_id,
            frames,
            &derived,
            options.workers,
            options.simd,
            &mut breakdown,
        );
    } else {
        process_source_batch(eval, &mut sf, source_id, frames, &derived, &mut breakdown);
    }
    // 服务耗时包含锁等待和未采样的工作，不把内部抽样分解当成整批耗时。
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

/// 兼容入口 — 记录 + 求值一次完成 (测试 / 同步 flush 路径; 运行时两平面分离)
///
/// 返回数值平面耗时 ns (push_frame + 图评估 + 派生 + 频谱, 观测用)。
pub fn on_frames(plane: &DataPlaneState, source_id: &str, frames: &[DataFrame]) -> u64 {
    record_frames(plane, source_id, frames);
    let buffer = plane.buffer_for(source_id);
    let options = EvalOptions::from_config(&plane.pipeline_config.read());
    eval_frames(
        &plane.eval,
        &plane.global_nodes,
        &buffer,
        source_id,
        frames,
        options,
    )
}

/// 帧分发主体 — 只依赖 eval 状态 + 全局节点表 + 该源 buffer (全部 Arc 可克隆)
///
/// 供把整段重型同步评估丢进 `tokio::task::spawn_blocking` 执行:
/// 大批次 (700k 时可达 24ms+) 不再占住 tokio worker, await 侧保持同源批序。
///
/// 两路分派 (见 [`EvalOptions`]): `workers ≥ 2` 走图内路径分块 fork-join 并行
/// 评估 ([`crate::graph_eval_parallel`], SIMD 开启时 Math 单元批量求值),
/// 1 = 串行热路径。
#[allow(clippy::implicit_hasher)] // 与 DataPlaneState.global_nodes 的具体 hasher 类型耦合, 泛化 S 会传染整个状态图
pub fn on_frames_detached(
    eval: &GraphEvalState,
    global_nodes: &Mutex<HashMap<String, NodeDef>>,
    buffer: &Arc<Mutex<DataBuffer>>,
    source_id: &str,
    frames: &[DataFrame],
    options: EvalOptions,
) -> u64 {
    eval_frames(eval, global_nodes, buffer, source_id, frames, options)
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
        // 降载发布: 步长抽点 + 恒含批尾最新值 (不变量 5 的降载可观测性由
        // 预览面板自身承担; 全量拷贝在 10M 帧/s 下不可行, 且是预览广播
        // 溢出 "样本预览跳过" 的压力来源)
        let stride = frames.len().div_ceil(PREVIEW_MAX_SAMPLES_PER_BATCH).max(1);
        let mut timestamps = Vec::with_capacity(frames.len() / stride + 1);
        let mut values = Vec::with_capacity(frames.len() / stride + 1);
        for (i, frame) in frames.iter().enumerate() {
            if i % stride != 0 && i + 1 != frames.len() {
                continue;
            }
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
/// 无缓存时槽位不写、快照保持上次值 (见 eval)。
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
