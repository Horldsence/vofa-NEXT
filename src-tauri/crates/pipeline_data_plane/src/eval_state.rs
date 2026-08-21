//! 共享状态类型 — GraphEvalState + StreamGroupState + 4 个 snapshot
//!
//! 这些类型服务于 AppState (Tauri-managed state) 与数据平面 (`DataPlaneState`),
//! 通过 `pipeline_data_plane` crate 同时被两者依赖, 打破潜在的环依赖。
//! 在 `src-tauri` 旧结构中, 它们定义于 `state::app_state.rs`。

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tauri::ipc::Channel;
use vofa_next_buffer::RawDataCollector;
use vofa_next_dsp::{DigitalFilter, IfftState, SpectrumAnalyzer, SpectrumResult};
use vofa_next_nodes::{CompiledGraph, FrameParser, SourceFramesMap};

/// 单个图输出快照 — 通过 Channel 推送到前端
///
/// values: widgetId -> portId -> value
/// 包含 ChannelSource/Input/Math/Custom/Filter 节点的输出
/// 前端通过 edges 自行解析 Sink 节点的输入 (上游 widgetId + sourceHandle)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphOutputSnapshot {
    /// 自增计数器, 前端可用于去重/丢弃过期帧
    pub tick: u64,
    /// 生成该快照时的图版本号 — 检测图重编译, 避免复用缓冲带来过期节点
    /// (仅后端内部使用, 不下发前端)
    #[serde(skip)]
    pub graphs_version: u64,
    /// widgetId -> portId -> value (FxHash 快速哈希表, 见 vofa_next_nodes::ValuesMap)
    pub values: vofa_next_nodes::ValuesMap,
}

/// Custom widget 输入批次 — 后端推送到前端 iframe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomInputBatch {
    /// custom widget id -> input port id -> value
    pub inputs: HashMap<String, HashMap<String, f32>>,
}

/// 字符串输出快照 — 与 graphOutputs 平行的字符串平面
///
/// Trigger widget 命中字符串类型规则时, 通过 `submit_custom_text_output` 写入;
/// 后端 ticker 把最新快照推给前端 (TextDisplay 控件读取显示)。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StringOutputSnapshot {
    /// 自增计数器, 与 GraphOutputSnapshot.tick 解耦 (独立推流节奏)
    pub tick: u64,
    /// widgetId -> portId -> string value
    pub values: HashMap<String, HashMap<String, String>>,
}

/// 频谱分析结果批次 — 后端推送到前端 SpectrumChart
///
/// 30 FPS 推送, key = SpectrumSink widget id, value = 最新一次 FFT 结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpectrumBatch {
    /// sink widget id -> 频谱结果
    pub spectra: HashMap<String, SpectrumResult>,
}

/// 流订阅组状态 — 统一分片框架 (`pipeline_stream`) 使用
///
/// 组内所有分片 (shard) 共享同一 seq 计数器 (在源锁内 fetch_add,
/// 保证全局单调且与 drain 顺序一致) 与同一流源实例 (游标类源的读游标在源内)。
pub struct StreamGroupState {
    /// 组级全局批次序号
    pub seq: Arc<AtomicU64>,
    /// 当前存活分片数 (归零时组被移除)
    pub shards: usize,
    /// 组共享流源 (Arc<Mutex<S>>, 加入组时按 S 类型 downcast 取回)
    pub source: Arc<dyn Any + Send + Sync>,
}

/// 节点图评估所需的共享状态 (从 AppState 抽取, 供数据平面/ticker 使用)
///
/// 设计动机: Tauri 2 的 State<'_, T> 内部是 &Arc<T> 但不暴露 Arc,
/// 我们也无法在 manage() 时包装 AppState 成 Arc<AppState> (因为 tauri::manage
/// 内部已用 Arc)。因此把数据平面需要的字段单独打包为 Arc, 从 AppState 克隆。
#[derive(Clone)]
pub struct GraphEvalState {
    pub graphs: Arc<Mutex<HashMap<String, CompiledGraph>>>,
    /// 图版本号 — sync_tab_graph/remove_tab_graph 时 +1,
    /// process_source_batch 据此检测重编译并清空复用的输出缓存
    pub graphs_version: Arc<AtomicU64>,
    pub input_values: Arc<Mutex<HashMap<String, f32>>>,
    pub custom_outputs: Arc<Mutex<HashMap<String, HashMap<String, f32>>>>,
    /// 每源最新帧缓存 (key = Protocol 节点 id, latest-value 融合) —
    /// 与 DataPlaneState::source_frames 共享同一 Arc (两平面衔接点)
    pub source_frames: Arc<Mutex<SourceFramesMap>>,
    pub output_snapshot: Arc<Mutex<GraphOutputSnapshot>>,
    pub output_subscribers: Arc<Mutex<Vec<Channel<GraphOutputSnapshot>>>>,
    pub custom_input_subscribers: Arc<Mutex<Vec<Channel<CustomInputBatch>>>>,
    /// 字符串输出 (Trigger 控件匹配字符串类型规则时写入)
    pub custom_text_outputs: Arc<Mutex<HashMap<String, HashMap<String, String>>>>,
    /// 字符串输出快照 (与 output_snapshot 平行, 由 text_output_ticker 推送)
    pub text_output_snapshot: Arc<Mutex<StringOutputSnapshot>>,
    /// 字符串输出订阅者
    pub text_output_subscribers: Arc<Mutex<Vec<Channel<StringOutputSnapshot>>>>,
    /// Filter 节点状态 (跨帧持久化, 逐点滤波)
    /// key: Filter widget id, value: DigitalFilter (含 FIR 延迟线 / IIR biquad 状态)
    pub filter_states: Arc<Mutex<HashMap<String, DigitalFilter>>>,
    /// FrameDecoder 节点状态 (跨帧持久化, 字节流解析状态机)
    /// key: FrameDecoder widget id, value: FrameParser (含 buf/state/last_frame)
    /// 由字节路由在命中 FrameDecoder 下游时经 feed_decoder_by_id 喂入
    pub decoder_states: Arc<Mutex<HashMap<String, FrameParser>>>,
    /// FrameDecoder 节点旁路原始字节收集器 (供前端 RawData 显示"每帧消费的原始字节")
    /// key: FrameDecoder widget id, value: Arc<Mutex<RawDataCollector>> (共享实例)
    /// 与 decoder_states 生命周期同步 (由 DecoderFeedCache::sync 增删),
    /// 独立于按 Transport 源的 raw_collectors (见 DataPlaneState)
    pub decoder_raw_collectors: Arc<Mutex<HashMap<String, Arc<Mutex<RawDataCollector>>>>>,
    /// SpectrumSink 节点对应的频谱分析器
    /// key: SpectrumSink widget id, value: SpectrumAnalyzer (含滑动窗口)
    /// 由 spectrum_ticker 在每 tick 开头与 graphs 同步 (增删)
    pub spectrum_analyzers: Arc<Mutex<HashMap<String, SpectrumAnalyzer>>>,
    /// 最新一次 FFT 结果 (供 30 FPS spectrum_ticker 推送)
    /// key: SpectrumSink widget id, value: SpectrumResult
    pub spectrum_snapshot: Arc<Mutex<HashMap<String, SpectrumResult>>>,
    /// 频谱订阅者 (30 FPS 推送 SpectrumBatch)
    pub spectrum_subscribers: Arc<Mutex<Vec<Channel<SpectrumBatch>>>>,
    /// Ifft 节点重建时域缓冲 (跨帧持久化, 环形播放)
    /// key: Ifft widget id, value: IfftState (含合成缓冲 + 播放位置)
    pub ifft_states: Arc<Mutex<HashMap<String, IfftState>>>,
}

/// `GraphEvalState` 构造: 把同一批 Arc 字段按下文约定批量装配
///
/// `AppState::new()` 与 `DataPlaneState::new()` 都调用此函数保证两份 `GraphEvalState`
/// (各自的字段值) 通过同一个 Arc 共享同源数据 (例如 `graphs` / `graphs_version` /
/// `source_frames` 等)。
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn build_graph_eval_state(
    graphs: Arc<Mutex<HashMap<String, CompiledGraph>>>,
    graphs_version: Arc<AtomicU64>,
    input_values: Arc<Mutex<HashMap<String, f32>>>,
    custom_outputs: Arc<Mutex<HashMap<String, HashMap<String, f32>>>>,
    text_output_snapshot: Arc<Mutex<StringOutputSnapshot>>,
    text_output_subscribers: Arc<Mutex<Vec<Channel<StringOutputSnapshot>>>>,
    custom_text_outputs: Arc<Mutex<HashMap<String, HashMap<String, String>>>>,
    source_frames: Arc<Mutex<SourceFramesMap>>,
    output_snapshot: Arc<Mutex<GraphOutputSnapshot>>,
    output_subscribers: Arc<Mutex<Vec<Channel<GraphOutputSnapshot>>>>,
    custom_input_subscribers: Arc<Mutex<Vec<Channel<CustomInputBatch>>>>,
    filter_states: Arc<Mutex<HashMap<String, DigitalFilter>>>,
    decoder_states: Arc<Mutex<HashMap<String, FrameParser>>>,
    decoder_raw_collectors: Arc<Mutex<HashMap<String, Arc<Mutex<RawDataCollector>>>>>,
    spectrum_analyzers: Arc<Mutex<HashMap<String, SpectrumAnalyzer>>>,
    spectrum_snapshot: Arc<Mutex<HashMap<String, SpectrumResult>>>,
    spectrum_subscribers: Arc<Mutex<Vec<Channel<SpectrumBatch>>>>,
    ifft_states: Arc<Mutex<HashMap<String, IfftState>>>,
) -> GraphEvalState {
    GraphEvalState {
        graphs,
        graphs_version,
        input_values,
        custom_outputs,
        custom_text_outputs,
        text_output_snapshot,
        text_output_subscribers,
        source_frames,
        output_snapshot,
        output_subscribers,
        custom_input_subscribers,
        filter_states,
        decoder_states,
        decoder_raw_collectors,
        spectrum_analyzers,
        spectrum_snapshot,
        spectrum_subscribers,
        ifft_states,
    }
}

/// 各通道默认缓冲区容量 (供 `AppState::new` 装配数据平面/状态时使用)
pub const DEFAULT_CAN_BUFFER_CAPACITY: usize = 50_000;
/// CAN 负载统计滑动窗口默认规格 (window_us, history_capacity) — 与 `CanLoadStats::new` 默认对齐
pub const DEFAULT_CAN_LOAD_STATS_WINDOW: (u64, usize) = (1_000_000, 120);
/// 逻辑采样缓冲区默认容量
pub const DEFAULT_LOGIC_BUFFER_CAPACITY: usize = 20_000;
/// 解码事件缓冲区默认容量
pub const DEFAULT_DECODED_BUFFER_CAPACITY: usize = 10_000;
